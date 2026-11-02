//! Generiek API-calling framework: `http_get`/`http_request` + per geconfigureerde API een tool
//! per endpoint (`config/apis.toml`), met secrets uit de broker (`secret:NAAM` in headers).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::web::{client, net_err};
use super::{BuiltinConfig, s};
use crate::contract::{Effect, ResourceUse, ToolContract};
use crate::error::ToolError;
use crate::tool::{Tool, ToolCtx, ToolOutput};

/// Resolver voor `secret:NAAM` (de hub koppelt hem aan de secrets broker).
pub type SecretFn = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

async fn send(
    http: &reqwest::Client,
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&serde_json::Value>,
) -> Result<ToolOutput, ToolError> {
    let m = reqwest::Method::from_bytes(method.to_uppercase().as_bytes())
        .map_err(|_| ToolError::invalid(format!("onbekende methode {method}")))?;
    let mut rb = http.request(m, url);
    for (k, v) in headers {
        rb = rb.header(k, v);
    }
    if let Some(b) = body {
        rb = rb.json(b);
    }
    let r = rb.send().await.map_err(net_err)?;
    let st = r.status();
    let ct = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let text = r.text().await.map_err(net_err)?;
    let data = serde_json::from_str::<serde_json::Value>(&text).unwrap_or(serde_json::Value::String(text.clone()));
    if st.is_server_error() || st.as_u16() == 429 {
        return Err(ToolError::transient(format!("HTTP {st}: {}", text.chars().take(300).collect::<String>())));
    }
    if !st.is_success() {
        return Err(ToolError::failed(format!("HTTP {st}: {}", text.chars().take(500).collect::<String>()))
            .with_hint("controleer URL, parameters en authenticatie"));
    }
    let shown = match &data {
        serde_json::Value::String(t) => t.clone(),
        v => serde_json::to_string_pretty(v).unwrap_or_default(),
    };
    Ok(ToolOutput {
        data: json!({"status": st.as_u16(), "content_type": ct, "body": data}),
        text: shown,
        preview: None,
    })
}

fn header_map(v: &serde_json::Value) -> BTreeMap<String, String> {
    v.get("headers")
        .and_then(|h| h.as_object())
        .map(|m| {
            m.iter().map(|(k, v)| (k.clone(), v.as_str().map(String::from).unwrap_or_else(|| v.to_string()))).collect()
        })
        .unwrap_or_default()
}

#[derive(Debug)]
pub struct HttpGet {
    c: ToolContract,
    http: reqwest::Client,
}
impl HttpGet {
    pub fn new(cfg: &BuiltinConfig) -> Self {
        Self {
            c: ToolContract::new(
                "http_get",
                "HTTP GET naar een API (JSON wordt netjes getoond); headers optioneel.",
                Effect::Read,
                json!({"type": "object", "required": ["url"], "properties": {"url": {"type": "string"}, "headers": {"type": "object"}}}),
            )
            .resources(vec![ResourceUse::Net])
            .net(vec!["*".into()])
            .cacheable()
            .timeout(30_000),
            http: client(cfg, Duration::from_secs(25)),
        }
    }
}
#[async_trait::async_trait]
impl Tool for HttpGet {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, _ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        send(&self.http, "GET", s(&i, "url"), &header_map(&i), None).await
    }
}

#[derive(Debug)]
pub struct HttpRequest {
    c: ToolContract,
    http: reqwest::Client,
}
impl HttpRequest {
    pub fn new(cfg: &BuiltinConfig) -> Self {
        Self {
            c: ToolContract::new(
                "http_request",
                "HTTP-verzoek met effect (POST/PUT/PATCH/DELETE) naar een API, JSON-body.",
                Effect::External,
                json!({"type": "object", "required": ["method", "url"], "properties": {
                    "method": {"type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"]},
                    "url": {"type": "string"}, "headers": {"type": "object"}, "body": {}
                }}),
            )
            .resources(vec![ResourceUse::Net])
            .net(vec!["*".into()])
            .timeout(60_000),
            http: client(cfg, Duration::from_secs(55)),
        }
    }
}
#[async_trait::async_trait]
impl Tool for HttpRequest {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, _ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        send(&self.http, s(&i, "method"), s(&i, "url"), &header_map(&i), i.get("body")).await
    }
}

/// Eén API uit `config/apis.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiSpec {
    pub base_url: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub endpoints: BTreeMap<String, EndpointSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointSpec {
    #[serde(default = "get")]
    pub method: String,
    /// Pad met `{param}`-plekken.
    pub path: String,
    #[serde(default)]
    pub description: String,
    /// Parameter → type (`string`, `number`, `integer`, `boolean`, `object`, `array`).
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    #[serde(default)]
    pub required: Vec<String>,
}
fn get() -> String {
    "GET".into()
}

#[derive(Debug)]
pub struct ApiEndpoint {
    c: ToolContract,
    api: ApiSpec,
    ep: EndpointSpec,
    http: reqwest::Client,
    secrets: SecretFnBox,
}

/// `Debug`-vriendelijke wrapper.
#[derive(Clone)]
pub struct SecretFnBox(pub SecretFn);
impl std::fmt::Debug for SecretFnBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretFn")
    }
}

impl ApiEndpoint {
    pub fn new(
        api_name: &str,
        ep_name: &str,
        api: ApiSpec,
        ep: EndpointSpec,
        cfg: &BuiltinConfig,
        secrets: SecretFn,
    ) -> Self {
        let mut props = serde_json::Map::new();
        for (k, t) in &ep.params {
            props.insert(k.clone(), json!({"type": t}));
        }
        let effect = if ep.method.eq_ignore_ascii_case("GET") { Effect::Read } else { Effect::External };
        let host = api.base_url.split("//").nth(1).and_then(|h| h.split('/').next()).unwrap_or("").to_string();
        let desc = if ep.description.is_empty() {
            format!("{} {} ({})", ep.method, ep.path, api.description)
        } else {
            ep.description.clone()
        };
        let mut c = ToolContract::new(
            &format!("api__{api_name}__{ep_name}"),
            &desc,
            effect,
            json!({"type": "object", "required": ep.required, "properties": props}),
        )
        .resources(vec![ResourceUse::Net])
        .net(vec![host])
        .origin(&format!("api:{api_name}"))
        .timeout(60_000);
        if effect == Effect::Read {
            c = c.cacheable();
        }
        Self { c, api, ep, http: client(cfg, Duration::from_secs(55)), secrets: SecretFnBox(secrets) }
    }
}

#[async_trait::async_trait]
impl Tool for ApiEndpoint {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, _ctx: &ToolCtx, i: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let mut path = self.ep.path.clone();
        let mut rest = serde_json::Map::new();
        if let Some(m) = i.as_object() {
            for (k, v) in m {
                let ph = format!("{{{k}}}");
                if path.contains(&ph) {
                    let val = v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
                    path = path.replace(&ph, &val);
                } else {
                    rest.insert(k.clone(), v.clone());
                }
            }
        }
        let mut headers = BTreeMap::new();
        for (k, v) in &self.api.headers {
            let val = match v.strip_prefix("secret:") {
                Some(name) => (self.secrets.0)(name).ok_or_else(|| {
                    ToolError::new(crate::error::ToolErrorKind::Precondition, format!("secret {name} ontbreekt"))
                        .with_hint("zet het secret in Instellingen → Secrets")
                })?,
                None => v.clone(),
            };
            headers.insert(k.clone(), val);
        }
        let mut url = format!("{}{}", self.api.base_url.trim_end_matches('/'), path);
        let get = self.ep.method.eq_ignore_ascii_case("GET");
        if get && !rest.is_empty() {
            let q: Vec<String> = rest
                .iter()
                .map(|(k, v)| {
                    format!("{}={}", enc(k), enc(&v.as_str().map(String::from).unwrap_or_else(|| v.to_string())))
                })
                .collect();
            url.push(if url.contains('?') { '&' } else { '?' });
            url.push_str(&q.join("&"));
        }
        let body = (!get && !rest.is_empty()).then_some(serde_json::Value::Object(rest));
        send(&self.http, &self.ep.method, &url, &headers, body.as_ref()).await
    }
}

fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

/// Tools voor alle API's uit de config.
pub fn api_tools(apis: &BTreeMap<String, ApiSpec>, cfg: &BuiltinConfig, secrets: SecretFn) -> Vec<Arc<dyn Tool>> {
    let mut out: Vec<Arc<dyn Tool>> = Vec::new();
    for (an, a) in apis {
        for (en, e) in &a.endpoints {
            out.push(Arc::new(ApiEndpoint::new(an, en, a.clone(), e.clone(), cfg, secrets.clone())));
        }
    }
    out
}
