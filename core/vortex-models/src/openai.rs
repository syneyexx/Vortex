//! OpenAI-compatibele adapter (v1-backend: `llama-server`). Chat, streaming, embeddings, rerank,
//! structured output (json_schema/grammar), reasoning_effort, metingen, annuleren via disconnect.
// VX:W02.02

use std::time::Duration;

use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::backend::*;
use crate::error::{ModelError, Result};
use crate::metrics::MetricsRecorder;
use crate::stream::{StopMatcher, ToolArgsAssembler, Utf8Decoder};
use crate::toolcall::{RawToolCall, ToolCallParser, parser_by_id};

/// Wat llama-server over zichzelf meldt (`GET /props`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServerProps {
    pub chat_template: String,
    pub n_ctx: Option<u64>,
    pub model_path: Option<String>,
    pub vision: bool,
}

#[derive(Debug)]
pub struct OpenAiBackend {
    name: String,
    base: String,
    model: String,
    client: reqwest::Client,
    caps: Capabilities,
    parser: Box<dyn ToolCallParser>,
}

/// Wat een llama-server maximaal kan (wordt doorsneden met wat het model in de registry claimt).
pub fn llama_server_caps() -> Capabilities {
    Capabilities {
        chat: true,
        streaming: true,
        tools: true,
        vision: true,
        embeddings: true,
        rerank: true,
        cancellation: true,
        logprobs: true,
        adapters: true,
        json_schema: true,
        grammar: true,
        kv_slots: true,
        reasoning_effort: true,
    }
}

impl OpenAiBackend {
    /// `declared` = capabilities uit de registry (wat het model kan).
    pub fn new(name: &str, base_url: &str, model: &str, declared: &Capabilities, parser_id: &str) -> Result<Self> {
        let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(3)).build()?;
        Ok(Self {
            name: name.into(),
            base: base_url.trim_end_matches('/').into(),
            model: model.into(),
            client,
            caps: declared.intersect(&llama_server_caps()),
            parser: parser_by_id(parser_id)?,
        })
    }

    pub fn parser_id(&self) -> &'static str {
        self.parser.id()
    }

    /// Haal server-eigenschappen op en scherp capabilities aan (template-contract).
    // VX:W02.04
    pub async fn probe(&mut self) -> Result<ServerProps> {
        let v: Value = self.get_json("/props").await?;
        let props = ServerProps {
            chat_template: v.get("chat_template").and_then(|t| t.as_str()).unwrap_or_default().to_string(),
            n_ctx: v.pointer("/default_generation_settings/n_ctx").and_then(|n| n.as_u64()),
            model_path: v.get("model_path").and_then(|p| p.as_str()).map(String::from),
            vision: v.pointer("/modalities/vision").and_then(|b| b.as_bool()).unwrap_or(false),
        };
        validate_template(&props, &mut self.caps)?;
        Ok(props)
    }

    /// Geladen LoRA-adapters (llama-server `GET /lora-adapters`): (id, pad, schaal).
    pub async fn lora_adapters(&self) -> Result<Vec<(u32, String, f32)>> {
        let v = self.get_json("/lora-adapters").await?;
        Ok(v.as_array()
            .map(|a| {
                a.iter()
                    .map(|x| {
                        (
                            x["id"].as_u64().unwrap_or(0) as u32,
                            x["path"].as_str().unwrap_or("").to_string(),
                            x["scale"].as_f64().unwrap_or(0.0) as f32,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Schalen van LoRA-adapters zetten (hot-swap, W13.04). Niet genoemde adapters → 0.
    pub async fn set_lora(&self, scales: &[(u32, f32)]) -> Result<()> {
        let body = Value::Array(scales.iter().map(|(id, s)| json!({"id": id, "scale": s})).collect());
        self.post("/lora-adapters", &body, &CancellationToken::new()).await?;
        Ok(())
    }

    async fn get_json(&self, path: &str) -> Result<Value> {
        let r = self.client.get(format!("{}{path}", self.base)).send().await?;
        let status = r.status();
        if !status.is_success() {
            return Err(ModelError::Backend { status: status.as_u16(), body: r.text().await.unwrap_or_default() });
        }
        Ok(r.json().await?)
    }

    async fn post(&self, path: &str, body: &Value, cancel: &CancellationToken) -> Result<reqwest::Response> {
        let fut = self.client.post(format!("{}{path}", self.base)).json(body).send();
        let r = tokio::select! {
            r = fut => r?,
            _ = cancel.cancelled() => return Err(ModelError::Cancelled),
        };
        let status = r.status();
        if !status.is_success() {
            return Err(ModelError::Backend { status: status.as_u16(), body: r.text().await.unwrap_or_default() });
        }
        Ok(r)
    }

    fn body(&self, req: &ChatRequest, stream: bool) -> Value {
        let messages: Vec<Value> = req
            .messages
            .iter()
            .map(|m| {
                let mut o = json!({"role": m.role, "content": m.content});
                if !m.images.is_empty() {
                    let mut parts = vec![json!({"type": "text", "text": m.content})];
                    parts.extend(m.images.iter().map(|u| json!({"type": "image_url", "image_url": {"url": u}})));
                    o["content"] = Value::Array(parts);
                }
                if !m.tool_calls.is_empty() {
                    o["tool_calls"] = m
                        .tool_calls
                        .iter()
                        .map(|c| json!({"id": c.id, "type": "function", "function": {"name": c.name, "arguments": c.arguments.to_string()}}))
                        .collect();
                }
                if let Some(id) = &m.tool_call_id {
                    o["tool_call_id"] = json!(id);
                }
                if let Some(n) = &m.name {
                    o["name"] = json!(n);
                }
                o
            })
            .collect();
        let mut b = json!({"model": self.model, "messages": messages, "stream": stream});
        // Prompt caching (W07.09): llama-server hergebruikt de KV-cache van de stabiele prefix.
        if self.caps.kv_slots {
            b["cache_prompt"] = json!(true);
        }
        if stream {
            b["stream_options"] = json!({"include_usage": true});
        }
        if !req.tools.is_empty() {
            b["tools"] = req
                .tools
                .iter()
                .map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.parameters}}))
                .collect();
        }
        if let Some(t) = req.temperature {
            b["temperature"] = json!(t);
        }
        if let Some(m) = req.max_tokens {
            b["max_tokens"] = json!(m);
        }
        if !req.stop.is_empty() {
            b["stop"] = json!(req.stop);
        }
        if let Some(s) = req.seed {
            b["seed"] = json!(s);
        }
        if req.logprobs {
            b["logprobs"] = json!(true);
        }
        if let Some(l) = &req.lora {
            // LoRA per verzoek (nieuwere llama-server); anders geldt de globale schaal.
            b["lora"] = Value::Array(l.iter().map(|(id, sc)| json!({"id": id, "scale": sc})).collect());
        }
        if let Some(e) = &req.reasoning_effort {
            b["reasoning_effort"] = json!(e);
            b["chat_template_kwargs"] = json!({"reasoning_effort": e});
        }
        match &req.response_format {
            Some(ResponseFormat::JsonObject) => b["response_format"] = json!({"type": "json_object"}),
            Some(ResponseFormat::JsonSchema { name, schema }) => {
                b["response_format"] =
                    json!({"type": "json_schema", "json_schema": {"name": name, "schema": schema, "strict": true}})
            }
            Some(ResponseFormat::Grammar { gbnf }) => b["grammar"] = json!(gbnf),
            Some(ResponseFormat::Text) | None => {}
        }
        b
    }
}

/// Template-contract: zonder template geen chat; zonder tool-rol in het template geen tools.
pub fn validate_template(props: &ServerProps, caps: &mut Capabilities) -> Result<()> {
    if props.chat_template.trim().is_empty() {
        return Err(ModelError::Lifecycle("server meldt geen chat-template (start llama-server met --jinja)".into()));
    }
    let t = &props.chat_template;
    let has_tools = t.contains("tool") || t.contains("function");
    if caps.tools && !has_tools {
        tracing::warn!("chat-template kent geen tool-berichten; tools uitgeschakeld voor dit model");
        caps.tools = false;
    }
    if !props.vision {
        caps.vision = false;
    }
    Ok(())
}

fn finite_check(v: &Value) -> Result<()> {
    // logprobs met NaN komen als null of als string "nan" binnen.
    if let Some(lp) = v.pointer("/choices/0/logprobs/content").and_then(|c| c.as_array()) {
        for t in lp {
            if let Some(x) = t.get("logprob")
                && (x.is_null() || x.as_str().is_some_and(|s| s.eq_ignore_ascii_case("nan") || s.contains("inf")))
            {
                return Err(ModelError::Numeric("logprob NaN/inf".into()));
            }
        }
    }
    Ok(())
}

fn raw_calls(v: Option<&Value>) -> Vec<RawToolCall> {
    v.and_then(|t| t.as_array())
        .map(|a| {
            a.iter()
                .map(|c| RawToolCall {
                    id: c.get("id").and_then(|x| x.as_str()).unwrap_or_default().into(),
                    name: c.pointer("/function/name").and_then(|x| x.as_str()).unwrap_or_default().into(),
                    arguments: c.pointer("/function/arguments").and_then(|x| x.as_str()).unwrap_or_default().into(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn usage(v: &Value) -> TokenUsage {
    TokenUsage {
        prompt_tokens: v.pointer("/usage/prompt_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
        completion_tokens: v.pointer("/usage/completion_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
    }
}

#[async_trait::async_trait]
impl Backend for OpenAiBackend {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> &Capabilities {
        &self.caps
    }

    async fn health(&self) -> Result<bool> {
        match self.client.get(format!("{}/health", self.base)).timeout(Duration::from_secs(2)).send().await {
            Ok(r) => Ok(r.status().is_success()),
            Err(e) if e.is_connect() || e.is_timeout() => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn chat(&self, req: &ChatRequest, cancel: &CancellationToken) -> Result<ChatResponse> {
        self.caps.check(req)?;
        let mut rec = MetricsRecorder::start(&self.model);
        let r = self.post("/v1/chat/completions", &self.body(req, false), cancel).await?;
        rec.first_byte();
        let body = tokio::select! {
            v = r.text() => v?,
            _ = cancel.cancelled() => return Err(ModelError::Cancelled),
        };
        let v: Value = serde_json::from_str(&body).map_err(|e| {
            ModelError::InvalidOutput(format!(
                "geen geldige JSON van de server ({e}): {}",
                body.chars().take(200).collect::<String>()
            ))
        })?;
        finite_check(&v)?;
        rec.token();
        if let Some(t) = v.get("timings") {
            rec.server_timings(t);
        }
        let msg = v.pointer("/choices/0/message").cloned().unwrap_or(Value::Null);
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or_default();
        let (content, tool_calls) = self.parser.parse(content, &raw_calls(msg.get("tool_calls")), &req.tools)?;
        let mut stop = StopMatcher::new(req.stop.clone());
        let o = stop.push(&content);
        let content = if o.stopped.is_some() { o.text } else { o.text + &stop.finish() };
        let usage = usage(&v);
        let mut metrics = rec.finish()?;
        metrics.prompt_tokens = metrics.prompt_tokens.max(usage.prompt_tokens);
        metrics.completion_tokens = metrics.completion_tokens.max(usage.completion_tokens);
        if let Some(ResponseFormat::JsonSchema { .. } | ResponseFormat::JsonObject) = &req.response_format {
            serde_json::from_str::<Value>(&content)
                .map_err(|e| ModelError::InvalidOutput(format!("geen geldige JSON: {e}")))?;
        }
        Ok(ChatResponse {
            content,
            reasoning: msg.get("reasoning_content").and_then(|c| c.as_str()).unwrap_or_default().into(),
            tool_calls,
            finish_reason: v.pointer("/choices/0/finish_reason").and_then(|f| f.as_str()).unwrap_or("stop").into(),
            usage,
            metrics,
        })
    }

    async fn chat_stream(
        &self,
        req: &ChatRequest,
        cancel: CancellationToken,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        self.caps.check(req)?;
        let mut rec = MetricsRecorder::start(&self.model);
        let resp = self.post("/v1/chat/completions", &self.body(req, true), &cancel).await?;
        rec.first_byte();
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<StreamEvent>>(64);
        let tools = req.tools.clone();
        let stops = req.stop.clone();
        let parser = parser_by_id(self.parser.id())?;
        tokio::spawn(async move {
            let mut bytes = resp.bytes_stream();
            let mut dec = Utf8Decoder::new();
            let mut line = String::new();
            let mut stop = StopMatcher::new(stops);
            let mut text_all = String::new();
            let mut calls: Vec<ToolArgsAssembler> = Vec::new();
            let mut finish = String::from("stop");
            let mut use_ = TokenUsage::default();
            let mut stopped = false;
            let send = |ev: Result<StreamEvent>| {
                let tx = tx.clone();
                async move { tx.send(ev).await.is_ok() }
            };
            'outer: loop {
                let chunk = tokio::select! {
                    c = bytes.next() => c,
                    _ = cancel.cancelled() => { let _ = send(Err(ModelError::Cancelled)).await; return; }
                };
                let Some(chunk) = chunk else { break };
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = send(Err(e.into())).await;
                        return;
                    }
                };
                line.push_str(&dec.push(&chunk));
                while let Some(nl) = line.find('\n') {
                    let l: String = line.drain(..=nl).collect();
                    let l = l.trim();
                    let Some(data) = l.strip_prefix("data:") else { continue };
                    let data = data.trim();
                    if data == "[DONE]" {
                        break 'outer;
                    }
                    let v: Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(e) => {
                            let _ = send(Err(ModelError::InvalidOutput(format!("SSE: {e}")))).await;
                            return;
                        }
                    };
                    if let Err(e) = finite_check(&v) {
                        let _ = send(Err(e)).await;
                        return;
                    }
                    if v.get("usage").is_some_and(|u| !u.is_null()) {
                        use_ = usage(&v);
                    }
                    if let Some(t) = v.get("timings") {
                        rec.server_timings(t);
                    }
                    if let Some(f) = v.pointer("/choices/0/finish_reason").and_then(|f| f.as_str()) {
                        finish = f.into();
                    }
                    let Some(delta) = v.pointer("/choices/0/delta") else { continue };
                    if let Some(r) = delta.get("reasoning_content").and_then(|r| r.as_str()).filter(|s| !s.is_empty()) {
                        rec.token();
                        if !send(Ok(StreamEvent::Reasoning(r.into()))).await {
                            return;
                        }
                    }
                    if let Some(c) = delta.get("content").and_then(|c| c.as_str()).filter(|s| !s.is_empty()) {
                        rec.token();
                        if stopped {
                            continue;
                        }
                        let o = stop.push(c);
                        text_all.push_str(&o.text);
                        if !o.text.is_empty() && !send(Ok(StreamEvent::Text(o.text))).await {
                            return;
                        }
                        if o.stopped.is_some() {
                            stopped = true;
                            finish = "stop".into();
                        }
                    }
                    if let Some(tc) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                        for c in tc {
                            let idx = c.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                            if calls.len() <= idx {
                                calls.resize_with(idx + 1, ToolArgsAssembler::default);
                            }
                            let name = c.pointer("/function/name").and_then(|n| n.as_str());
                            calls[idx].push(
                                c.get("id").and_then(|i| i.as_str()),
                                name,
                                c.pointer("/function/arguments").and_then(|a| a.as_str()).unwrap_or_default(),
                            );
                            if !send(Ok(StreamEvent::ToolCallStreaming { index: idx, name: calls[idx].name.clone() }))
                                .await
                            {
                                return;
                            }
                        }
                    }
                }
            }
            if !stopped {
                let rest = stop.finish();
                if !rest.is_empty() {
                    text_all.push_str(&rest);
                    let _ = send(Ok(StreamEvent::Text(rest))).await;
                }
            }
            // Toolaanroepen pas na het einde valideren en vrijgeven.
            let raw: Vec<RawToolCall> = calls
                .iter()
                .map(|c| RawToolCall {
                    id: c.id.clone().unwrap_or_default(),
                    name: c.name.clone().unwrap_or_default(),
                    arguments: c.raw().to_string(),
                })
                .collect();
            match parser.parse(&text_all, &raw, &tools) {
                Ok((_, ready)) => {
                    for c in ready {
                        if !send(Ok(StreamEvent::ToolCallReady(c))).await {
                            return;
                        }
                    }
                }
                Err(e) => {
                    let _ = send(Err(e)).await;
                    return;
                }
            }
            match rec.finish() {
                Ok(mut m) => {
                    m.prompt_tokens = m.prompt_tokens.max(use_.prompt_tokens);
                    m.completion_tokens = m.completion_tokens.max(use_.completion_tokens);
                    let _ = send(Ok(StreamEvent::Done { finish_reason: finish, usage: use_, metrics: m })).await;
                }
                Err(e) => {
                    let _ = send(Err(e)).await;
                }
            }
        });
        Ok(futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|v| (v, rx)) }).boxed())
    }

    async fn embed(&self, texts: &[String], cancel: &CancellationToken) -> Result<Vec<Vec<f32>>> {
        if !self.caps.embeddings {
            return Err(ModelError::Unsupported("embeddings".into()));
        }
        let r = self.post("/v1/embeddings", &json!({"model": self.model, "input": texts}), cancel).await?;
        let v: Value = r.json().await?;
        let mut data: Vec<(usize, Vec<f32>)> = v
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| ModelError::InvalidOutput("embeddings: geen data".into()))?
            .iter()
            .map(|d| {
                let idx = d.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                let emb: Vec<f32> = d
                    .get("embedding")
                    .and_then(|e| e.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_f64()).map(|x| x as f32).collect())
                    .unwrap_or_default();
                (idx, emb)
            })
            .collect();
        data.sort_by_key(|(i, _)| *i);
        if data.len() != texts.len() {
            return Err(ModelError::InvalidOutput(format!(
                "embeddings: {} terug voor {} teksten",
                data.len(),
                texts.len()
            )));
        }
        for (_, e) in &data {
            if e.is_empty() || e.iter().any(|x| !x.is_finite()) {
                return Err(ModelError::Numeric("embedding leeg of NaN/inf".into()));
            }
        }
        Ok(data.into_iter().map(|(_, e)| e).collect())
    }

    async fn rerank(&self, query: &str, docs: &[String], cancel: &CancellationToken) -> Result<Vec<RerankScore>> {
        if !self.caps.rerank {
            return Err(ModelError::Unsupported("rerank".into()));
        }
        let r =
            self.post("/v1/rerank", &json!({"model": self.model, "query": query, "documents": docs}), cancel).await?;
        let v: Value = r.json().await?;
        let mut out: Vec<RerankScore> = v
            .get("results")
            .and_then(|d| d.as_array())
            .ok_or_else(|| ModelError::InvalidOutput("rerank: geen results".into()))?
            .iter()
            .filter_map(|d| {
                Some(RerankScore {
                    index: d.get("index")?.as_u64()? as usize,
                    score: d.get("relevance_score")?.as_f64()? as f32,
                })
            })
            .collect();
        if out.iter().any(|s| !s.score.is_finite() || s.index >= docs.len()) {
            return Err(ModelError::InvalidOutput("rerank: ongeldige score of index".into()));
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(out)
    }
}
