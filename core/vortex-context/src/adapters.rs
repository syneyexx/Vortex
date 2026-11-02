//! Koppeling geheugen ⇄ modellen: embeddings (1060), reranker (1060) en het router-model voor
//! contextbeschrijving, extractie, samenvatten en query-varianten (gestructureerde uitvoer).
//! Valt een model weg, dan valt het geheugen terug op de heuristiek (nooit stilzwijgend: tracing).
// VX:W05.03

use std::sync::Arc;

use serde_json::json;
use tokio_util::sync::CancellationToken;
use vortex_memory::llm::{Extraction, HeuristicLlm, MemLlm};
use vortex_memory::{Embedder, MemError, Reranker};
use vortex_models::backend::{Backend, ChatRequest, Message, ResponseFormat};

fn me(e: impl std::fmt::Display) -> MemError {
    MemError::Llm(e.to_string())
}

/// Embeddings via een backend (Qwen3-Embedding op de 1060).
#[derive(Debug)]
pub struct ModelEmbedder {
    backend: Arc<dyn Backend>,
    model: String,
    dims: usize,
}

impl ModelEmbedder {
    pub fn new(backend: Arc<dyn Backend>, model: &str, dims: usize) -> Self {
        Self { backend, model: model.into(), dims }
    }
}

#[async_trait::async_trait]
impl Embedder for ModelEmbedder {
    fn model(&self) -> &str {
        &self.model
    }
    fn dims(&self) -> usize {
        self.dims
    }
    async fn embed(&self, texts: &[String]) -> vortex_memory::Result<Vec<Vec<f32>>> {
        self.backend.embed(texts, &CancellationToken::new()).await.map_err(|e| MemError::Embed(e.to_string()))
    }
}

/// Reranker via een backend (Qwen3-Reranker op de 1060).
#[derive(Debug)]
pub struct ModelReranker(pub Arc<dyn Backend>);

#[async_trait::async_trait]
impl Reranker for ModelReranker {
    async fn rerank(&self, query: &str, docs: &[String]) -> vortex_memory::Result<Vec<f32>> {
        let r = self.0.rerank(query, docs, &CancellationToken::new()).await.map_err(me)?;
        let mut out = vec![0f32; docs.len()];
        for s in r {
            if s.index < out.len() {
                out[s.index] = s.score;
            }
        }
        Ok(out)
    }
}

/// Router-model (klein, snel) voor geheugentaken, met heuristische terugval.
#[derive(Debug)]
pub struct ModelMemLlm {
    backend: Arc<dyn Backend>,
    fallback: HeuristicLlm,
}

impl ModelMemLlm {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self { backend, fallback: HeuristicLlm }
    }

    async fn ask(
        &self,
        system: &str,
        user: String,
        schema: Option<serde_json::Value>,
        max_tokens: u32,
    ) -> Result<String, MemError> {
        let mut req = ChatRequest::new(vec![Message::system(system), Message::user(user)]);
        req.temperature = Some(0.1);
        req.max_tokens = Some(max_tokens);
        if let Some(s) = schema {
            req.response_format = Some(ResponseFormat::JsonSchema { name: "out".into(), schema: s });
        }
        Ok(self.backend.chat(&req, &CancellationToken::new()).await.map_err(me)?.content)
    }
}

#[async_trait::async_trait]
impl MemLlm for ModelMemLlm {
    async fn describe_context(&self, doc_title: &str, chunk: &str) -> vortex_memory::Result<String> {
        let snippet: String = chunk.chars().take(1500).collect();
        match self
            .ask(
                "Geef in één korte zin (max 25 woorden) waar dit fragment over gaat binnen het document. Alleen de zin.",
                format!("Document: {doc_title}\nFragment:\n{snippet}"),
                None,
                60,
            )
            .await
        {
            Ok(s) if !s.trim().is_empty() => Ok(s.trim().to_string()),
            other => {
                tracing::debug!(?other, "router-model niet bruikbaar voor context; heuristiek");
                self.fallback.describe_context(doc_title, chunk).await
            }
        }
    }

    async fn extract(&self, text: &str) -> vortex_memory::Result<Extraction> {
        let schema = json!({
            "type": "object",
            "properties": {
                "entities": {"type": "array", "items": {"type": "object", "properties": {"name": {"type": "string"}, "type": {"type": "string"}}, "required": ["name"]}},
                "claims": {"type": "array", "items": {"type": "object", "properties": {
                    "subject": {"type": "string"}, "predicate": {"type": "string"}, "object": {"type": "string"},
                    "polarity": {"type": "integer"}, "value": {"type": ["number", "null"]}, "unit": {"type": ["string", "null"]}},
                    "required": ["subject", "predicate", "object"]}}
            },
            "required": ["entities", "claims"]
        });
        let snippet: String = text.chars().take(3000).collect();
        match self
            .ask(
                "Extraheer entiteiten (personen, organisaties, plaatsen, producten, begrippen) en feitelijke claims (onderwerp, predicaat, object; polarity -1 bij ontkenning; value/unit bij getallen). Gebruik de taal van de tekst. Alleen JSON.",
                snippet,
                Some(schema),
                600,
            )
            .await
            .and_then(|s| serde_json::from_str::<Extraction>(&s).map_err(me))
        {
            Ok(x) => Ok(x),
            Err(e) => {
                tracing::debug!(error = %e, "extractie via model mislukt; heuristiek");
                self.fallback.extract(text).await
            }
        }
    }

    async fn summarize(&self, texts: &[String], max_words: usize) -> vortex_memory::Result<String> {
        let joined: String = texts.join("\n---\n").chars().take(6000).collect();
        match self
            .ask(
                &format!("Vat samen in maximaal {max_words} woorden. Feitelijk, geen inleiding."),
                joined,
                None,
                (max_words * 2) as u32,
            )
            .await
        {
            Ok(s) if !s.trim().is_empty() => Ok(s.trim().to_string()),
            _ => self.fallback.summarize(texts, max_words).await,
        }
    }

    async fn rewrite_query(&self, q: &str) -> vortex_memory::Result<Vec<String>> {
        let schema = json!({"type": "object", "properties": {"queries": {"type": "array", "items": {"type": "string"}, "maxItems": 3}}, "required": ["queries"]});
        let mut out = vec![q.to_string()];
        if let Ok(s) = self
            .ask(
                "Geef maximaal 3 korte alternatieve zoekvragen (synoniemen, andere formulering, kernbegrippen) voor geheugenzoeken. Alleen JSON.",
                q.to_string(),
                Some(schema),
                120,
            )
            .await
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&s)
        {
            out.extend(v["queries"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).map(String::from).take(3));
        }
        Ok(out)
    }
}
