//! MCP-client (W08.08): stdio (subproces, JSON-RPC per regel) en HTTP ("streamable HTTP":
//! JSON of SSE-antwoord, `Mcp-Session-Id`). Tools van de server komen in de registry als
//! `mcp__<server>__<tool>`; effect uit de annotaties (readOnlyHint/destructiveHint/openWorldHint).
//! Valt een server weg, dan wordt bij de volgende aanroep opnieuw verbonden.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

use crate::contract::{Effect, ResourceUse, ToolContract};
use crate::error::{ToolError, ToolErrorKind};
use crate::registry::ToolRegistry;
use crate::tool::{Tool, ToolCtx, ToolOutput};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct McpServerCfg {
    #[serde(default)]
    pub name: String,
    /// stdio: programma + argumenten.
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub cwd: Option<String>,
    /// HTTP: endpoint-URL.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "yes")]
    pub enabled: bool,
}
fn default_timeout() -> u64 {
    60_000
}
fn yes() -> bool {
    true
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

#[derive(Debug)]
struct StdioConn {
    _child: tokio::process::Child,
    stdin: tokio::sync::Mutex<tokio::process::ChildStdin>,
    pending: Pending,
    alive: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Debug)]
struct HttpConn {
    http: reqwest::Client,
    url: String,
    headers: BTreeMap<String, String>,
    session: Mutex<Option<String>>,
}

#[derive(Debug)]
enum Conn {
    Stdio(StdioConn),
    Http(HttpConn),
}

/// Toolbeschrijving zoals de server hem geeft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(default)]
    pub annotations: Value,
}

#[derive(Debug)]
pub struct McpClient {
    pub cfg: McpServerCfg,
    conn: tokio::sync::Mutex<Option<Arc<Conn>>>,
    next_id: AtomicU64,
    pub server_info: Mutex<Value>,
}

fn rpc_error(v: &Value) -> Option<ToolError> {
    v.get("error")
        .map(|e| ToolError::failed(format!("MCP-fout {}: {}", e["code"], e["message"].as_str().unwrap_or("onbekend"))))
}

impl McpClient {
    pub fn new(cfg: McpServerCfg) -> Self {
        Self {
            cfg,
            conn: tokio::sync::Mutex::new(None),
            next_id: AtomicU64::new(1),
            server_info: Mutex::new(Value::Null),
        }
    }

    async fn open(&self) -> Result<Arc<Conn>, ToolError> {
        let conn = if let Some(cmd) = &self.cfg.command {
            let mut c = tokio::process::Command::new(cmd);
            c.args(&self.cfg.args)
                .envs(&self.cfg.env)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true);
            if let Some(d) = &self.cfg.cwd {
                c.current_dir(d);
            }
            let mut child = c
                .spawn()
                .map_err(|e| ToolError::new(ToolErrorKind::Transient, format!("MCP {} starten: {e}", self.cfg.name)))?;
            let stdin = child.stdin.take().ok_or_else(|| ToolError::failed("geen stdin"))?;
            let stdout = child.stdout.take().ok_or_else(|| ToolError::failed("geen stdout"))?;
            let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
            let alive = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let (p2, a2) = (pending.clone(), alive.clone());
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                    // Alleen antwoorden (met id en result/error); notificaties/verzoeken van de server negeren.
                    if (v.get("result").is_some() || v.get("error").is_some())
                        && let Some(id) = v.get("id").and_then(|i| i.as_u64())
                        && let Some(tx) = p2.lock().remove(&id)
                    {
                        let _ = tx.send(v);
                    }
                }
                a2.store(false, Ordering::SeqCst);
                p2.lock().clear();
            });
            Conn::Stdio(StdioConn { _child: child, stdin: tokio::sync::Mutex::new(stdin), pending, alive })
        } else if let Some(url) = &self.cfg.url {
            Conn::Http(HttpConn {
                http: reqwest::Client::builder()
                    .timeout(Duration::from_millis(self.cfg.timeout_ms))
                    .build()
                    .unwrap_or_default(),
                url: url.clone(),
                headers: self.cfg.headers.clone(),
                session: Mutex::new(None),
            })
        } else {
            return Err(ToolError::invalid(format!("MCP-server {} heeft geen command of url", self.cfg.name)));
        };
        let conn = Arc::new(conn);
        // Handdruk.
        let init = self
            .send(
                &conn,
                "initialize",
                json!({"protocolVersion": PROTOCOL_VERSION, "capabilities": {}, "clientInfo": {"name": "vortex", "version": env!("CARGO_PKG_VERSION")}}),
            )
            .await?;
        *self.server_info.lock() = init.get("serverInfo").cloned().unwrap_or(Value::Null);
        self.notify(&conn, "notifications/initialized", json!({})).await?;
        Ok(conn)
    }

    async fn ensure(&self) -> Result<Arc<Conn>, ToolError> {
        let mut g = self.conn.lock().await;
        if let Some(c) = g.as_ref() {
            let alive = match c.as_ref() {
                Conn::Stdio(s) => s.alive.load(Ordering::SeqCst),
                Conn::Http(_) => true,
            };
            if alive {
                return Ok(c.clone());
            }
        }
        let c = self.open().await?;
        *g = Some(c.clone());
        Ok(c)
    }

    async fn notify(&self, conn: &Conn, method: &str, params: Value) -> Result<(), ToolError> {
        let msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
        match conn {
            Conn::Stdio(s) => {
                let mut w = s.stdin.lock().await;
                w.write_all(format!("{msg}\n").as_bytes()).await.map_err(|e| ToolError::transient(e.to_string()))?;
                w.flush().await.map_err(|e| ToolError::transient(e.to_string()))?;
                Ok(())
            }
            Conn::Http(h) => {
                let _ = self.http_post(h, &msg).await;
                Ok(())
            }
        }
    }

    async fn http_post(&self, h: &HttpConn, msg: &Value) -> Result<Option<Value>, ToolError> {
        let mut rb = h
            .http
            .post(&h.url)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", PROTOCOL_VERSION)
            .json(msg);
        for (k, v) in &h.headers {
            rb = rb.header(k, v);
        }
        if let Some(sid) = h.session.lock().clone() {
            rb = rb.header("Mcp-Session-Id", sid);
        }
        let r = rb.send().await.map_err(|e| ToolError::transient(format!("MCP {}: {e}", self.cfg.name)))?;
        if let Some(sid) = r.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            *h.session.lock() = Some(sid.to_string());
        }
        let st = r.status();
        if st.as_u16() == 202 || st.as_u16() == 204 {
            return Ok(None);
        }
        if !st.is_success() {
            return Err(if st.is_server_error() {
                ToolError::transient(format!("MCP HTTP {st}"))
            } else {
                ToolError::failed(format!("MCP HTTP {st}"))
            });
        }
        let ct = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let body = r.text().await.map_err(|e| ToolError::transient(e.to_string()))?;
        if ct.contains("event-stream") {
            // SSE: laatste data-regel met ons antwoord.
            let want = msg.get("id").cloned();
            for ev in body.split("\n\n") {
                let data: String =
                    ev.lines().filter_map(|l| l.strip_prefix("data:")).map(str::trim).collect::<Vec<_>>().join("\n");
                if let Ok(v) = serde_json::from_str::<Value>(&data)
                    && v.get("id") == want.as_ref()
                {
                    return Ok(Some(v));
                }
            }
            return Err(ToolError::failed("MCP: geen antwoord in SSE-stroom"));
        }
        if body.trim().is_empty() {
            return Ok(None);
        }
        serde_json::from_str(&body).map(Some).map_err(|e| ToolError::failed(format!("MCP: ongeldige JSON ({e})")))
    }

    async fn send(&self, conn: &Conn, method: &str, params: Value) -> Result<Value, ToolError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let timeout = Duration::from_millis(self.cfg.timeout_ms);
        let v = match conn {
            Conn::Stdio(s) => {
                let (tx, rx) = oneshot::channel();
                s.pending.lock().insert(id, tx);
                {
                    let mut w = s.stdin.lock().await;
                    w.write_all(format!("{msg}\n").as_bytes())
                        .await
                        .map_err(|e| ToolError::transient(format!("MCP {} weg: {e}", self.cfg.name)))?;
                    w.flush().await.map_err(|e| ToolError::transient(e.to_string()))?;
                }
                match tokio::time::timeout(timeout, rx).await {
                    Ok(Ok(v)) => v,
                    Ok(Err(_)) => return Err(ToolError::transient(format!("MCP-server {} gestopt", self.cfg.name))),
                    Err(_) => {
                        s.pending.lock().remove(&id);
                        return Err(ToolError::new(ToolErrorKind::Timeout, format!("MCP {method}")));
                    }
                }
            }
            Conn::Http(h) => self.http_post(h, &msg).await?.ok_or_else(|| ToolError::failed("MCP: leeg antwoord"))?,
        };
        if let Some(e) = rpc_error(&v) {
            return Err(e);
        }
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Verzoek met automatische herverbinding (één keer).
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, ToolError> {
        let conn = self.ensure().await?;
        match self.send(&conn, method, params.clone()).await {
            Err(e) if e.kind == ToolErrorKind::Transient => {
                *self.conn.lock().await = None;
                let conn = self.ensure().await?;
                self.send(&conn, method, params).await
            }
            other => other,
        }
    }

    pub async fn list_tools(&self) -> Result<Vec<McpToolDef>, ToolError> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..50 {
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let r = self.request("tools/list", params).await?;
            if let Some(a) = r.get("tools").and_then(|t| t.as_array()) {
                out.extend(a.iter().filter_map(|t| serde_json::from_value::<McpToolDef>(t.clone()).ok()));
            }
            cursor = r.get("nextCursor").and_then(|c| c.as_str()).map(String::from);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    pub async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput, ToolError> {
        let r = self.request("tools/call", json!({"name": name, "arguments": args})).await?;
        let mut text = String::new();
        if let Some(items) = r.get("content").and_then(|c| c.as_array()) {
            for it in items {
                match it.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        text.push_str(it["text"].as_str().unwrap_or(""));
                        text.push('\n');
                    }
                    Some("resource") => {
                        let res = &it["resource"];
                        text.push_str(res["text"].as_str().unwrap_or(res["uri"].as_str().unwrap_or("")));
                        text.push('\n');
                    }
                    Some(t) => text.push_str(&format!("[{t}-inhoud]\n")),
                    None => {}
                }
            }
        }
        let text = text.trim_end().to_string();
        if r.get("isError").and_then(|e| e.as_bool()) == Some(true) {
            return Err(ToolError::failed(if text.is_empty() { "MCP-tool meldde een fout".into() } else { text }));
        }
        let data = r.get("structuredContent").cloned().unwrap_or_else(|| Value::String(text.clone()));
        Ok(ToolOutput { data, text, preview: None })
    }
}

/// Een MCP-tool als Vortex-tool.
#[derive(Debug)]
pub struct McpTool {
    c: ToolContract,
    remote: String,
    client: Arc<McpClient>,
}

impl McpTool {
    pub fn new(server: &str, def: &McpToolDef, client: Arc<McpClient>) -> Self {
        let ann = &def.annotations;
        let ro = ann.get("readOnlyHint").and_then(|v| v.as_bool()) == Some(true);
        let open = ann.get("openWorldHint").and_then(|v| v.as_bool()).unwrap_or(false);
        let effect = if ro {
            Effect::Read
        } else if open {
            Effect::External
        } else {
            Effect::Local
        };
        let schema = if def.input_schema.is_object() { def.input_schema.clone() } else { json!({"type": "object"}) };
        let safe: String =
            def.name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
        let mut c = ToolContract::new(
            &format!("mcp__{server}__{safe}"),
            def.description.as_deref().unwrap_or(&def.name).lines().next().unwrap_or(""),
            effect,
            schema,
        )
        .origin(&format!("mcp:{server}"))
        .resources(vec![ResourceUse::Named(format!("mcp:{server}"))])
        .timeout(client.cfg.timeout_ms);
        c.idempotent = ro || ann.get("idempotentHint").and_then(|v| v.as_bool()) == Some(true);
        Self { c, remote: def.name.clone(), client }
    }
}

#[async_trait::async_trait]
impl Tool for McpTool {
    fn contract(&self) -> &ToolContract {
        &self.c
    }
    async fn call(&self, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
        tokio::select! {
            r = self.client.call_tool(&self.remote, input) => r,
            _ = ctx.cancel.cancelled() => {
                // Annuleren doorgeven (best effort).
                Err(ToolError::new(ToolErrorKind::Cancelled, "geannuleerd"))
            }
        }
    }
}

/// Alle MCP-servers uit de config.
#[derive(Debug, Default)]
pub struct McpManager {
    clients: Mutex<BTreeMap<String, Arc<McpClient>>>,
    status: Mutex<BTreeMap<String, String>>,
}

impl McpManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Verbind met alle servers en registreer hun tools. Fouten per server worden onthouden
    /// (status), andere servers gaan door.
    pub async fn connect_all(
        &self,
        cfgs: &BTreeMap<String, McpServerCfg>,
        reg: &ToolRegistry,
    ) -> BTreeMap<String, String> {
        for (name, cfg) in cfgs {
            reg.unregister_origin(&format!("mcp:{name}"));
            if !cfg.enabled {
                self.status.lock().insert(name.clone(), "uit".into());
                continue;
            }
            let mut cfg = cfg.clone();
            cfg.name = name.clone();
            let client = Arc::new(McpClient::new(cfg));
            let st = match client.list_tools().await {
                Ok(defs) => {
                    for d in &defs {
                        reg.register(Arc::new(McpTool::new(name, d, client.clone())));
                    }
                    format!("verbonden — {} tools", defs.len())
                }
                Err(e) => format!("fout: {}", e.message),
            };
            self.clients.lock().insert(name.clone(), client);
            self.status.lock().insert(name.clone(), st);
        }
        self.status()
    }

    pub fn status(&self) -> BTreeMap<String, String> {
        self.status.lock().clone()
    }

    pub fn client(&self, name: &str) -> Option<Arc<McpClient>> {
        self.clients.lock().get(name).cloned()
    }
}
