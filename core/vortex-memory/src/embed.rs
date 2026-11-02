//! Embeddings: trait + micro-batching + single-flight (W05.03). De echte embedder (1060-server)
//! wordt in `vortex-context` gekoppeld; hier ook een deterministische hash-embedder (offline
//! terugval en tests).
// VX:W05.03

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use crate::error::{MemError, Result};
use crate::vector::normalize;

#[async_trait::async_trait]
pub trait Embedder: Send + Sync + std::fmt::Debug {
    /// Model-ID (komt in de index-generatie).
    fn model(&self) -> &str;
    fn dims(&self) -> usize;
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// Deterministische "bag of words"-embedder met feature hashing. Geen model nodig; gebruikt
/// als terugval als de 1060 niet draait, en in tests.
#[derive(Debug, Clone)]
pub struct HashEmbedder {
    dims: usize,
}

impl HashEmbedder {
    pub fn new(dims: usize) -> Self {
        Self { dims }
    }
    fn one(&self, t: &str) -> Vec<f32> {
        let mut v = vec![0f32; self.dims];
        let lower = t.to_lowercase();
        let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 1).collect();
        for w in &words {
            let h = vortex_core::config::fnv1a64(w.as_bytes());
            let i = (h % self.dims as u64) as usize;
            v[i] += if (h >> 63) == 0 { 1.0 } else { -1.0 };
        }
        for pair in words.windows(2) {
            let h = vortex_core::config::fnv1a64(format!("{} {}", pair[0], pair[1]).as_bytes());
            v[(h % self.dims as u64) as usize] += 0.5;
        }
        normalize(v, self.dims)
    }
}

#[async_trait::async_trait]
impl Embedder for HashEmbedder {
    fn model(&self) -> &str {
        "hash-bow/v1"
    }
    fn dims(&self) -> usize {
        self.dims
    }
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.one(t)).collect())
    }
}

type Req = (String, oneshot::Sender<std::result::Result<Vec<f32>, String>>);

/// Verzamelt losse verzoeken tot batches (max `max_batch` of `window`) en dedupliceert
/// identieke teksten binnen een batch (single-flight). Resultaten worden ingekort/genormaliseerd
/// tot `dims` (MRL).
#[derive(Debug, Clone)]
pub struct BatchingEmbedder {
    tx: mpsc::Sender<Req>,
    model: Arc<str>,
    dims: usize,
}

impl BatchingEmbedder {
    pub fn new(inner: Arc<dyn Embedder>, dims: usize, max_batch: usize, window: Duration) -> Self {
        let (tx, mut rx) = mpsc::channel::<Req>(1024);
        let model: Arc<str> = format!("{}@{dims}", inner.model()).into();
        tokio::spawn(async move {
            while let Some(first) = rx.recv().await {
                let mut batch = vec![first];
                let deadline = tokio::time::Instant::now() + window;
                while batch.len() < max_batch {
                    match tokio::time::timeout_at(deadline, rx.recv()).await {
                        Ok(Some(r)) => batch.push(r),
                        _ => break,
                    }
                }
                let mut uniq: Vec<String> = Vec::new();
                let mut idx: HashMap<String, usize> = HashMap::new();
                for (t, _) in &batch {
                    if !idx.contains_key(t) {
                        idx.insert(t.clone(), uniq.len());
                        uniq.push(t.clone());
                    }
                }
                match inner.embed(&uniq).await {
                    Ok(vs) if vs.len() == uniq.len() => {
                        for (t, tx) in batch {
                            let v = vs[idx[&t]].clone();
                            let _ = tx.send(Ok(normalize(v, dims)));
                        }
                    }
                    Ok(vs) => {
                        for (_, tx) in batch {
                            let _ =
                                tx.send(Err(format!("embedder gaf {} vectoren voor {} teksten", vs.len(), uniq.len())));
                        }
                    }
                    Err(e) => {
                        for (_, tx) in batch {
                            let _ = tx.send(Err(e.to_string()));
                        }
                    }
                }
            }
        });
        Self { tx, model, dims }
    }

    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let (tx, rx) = oneshot::channel();
        self.tx.send((text.to_string(), tx)).await.map_err(|_| MemError::Embed("embedder gestopt".into()))?;
        rx.await.map_err(|_| MemError::Embed("embedder gestopt".into()))?.map_err(MemError::Embed)
    }
}

#[async_trait::async_trait]
impl Embedder for BatchingEmbedder {
    fn model(&self) -> &str {
        &self.model
    }
    fn dims(&self) -> usize {
        self.dims
    }
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let futs = texts.iter().map(|t| self.embed_one(t));
        futures_join(futs).await
    }
}

async fn futures_join<F, T>(futs: impl Iterator<Item = F>) -> Result<Vec<T>>
where
    F: std::future::Future<Output = Result<T>>,
{
    let mut out = Vec::new();
    let hs: Vec<F> = futs.collect();
    for f in hs {
        out.push(f.await?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct Counting(AtomicUsize, AtomicUsize);
    #[async_trait::async_trait]
    impl Embedder for Counting {
        fn model(&self) -> &str {
            "c"
        }
        fn dims(&self) -> usize {
            8
        }
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            self.1.fetch_add(texts.len(), Ordering::SeqCst);
            Ok(texts.iter().map(|t| vec![t.len() as f32; 8]).collect())
        }
    }

    #[tokio::test]
    async fn batches_and_dedupes_concurrent_requests() -> Result<()> {
        let inner = Arc::new(Counting(AtomicUsize::new(0), AtomicUsize::new(0)));
        let b = BatchingEmbedder::new(inner.clone(), 4, 32, Duration::from_millis(10));
        let mut hs = Vec::new();
        for i in 0..20 {
            let b = b.clone();
            hs.push(tokio::spawn(async move { b.embed_one(if i % 2 == 0 { "zelfde" } else { "anders" }).await }));
        }
        for h in hs {
            let v = h.await.map_err(|e| MemError::Embed(e.to_string()))??;
            assert_eq!(v.len(), 4, "ingekort tot dims");
        }
        assert_eq!(inner.0.load(Ordering::SeqCst), 1, "één batch");
        assert_eq!(inner.1.load(Ordering::SeqCst), 2, "dubbele teksten één keer");
        Ok(())
    }

    #[tokio::test]
    async fn hash_embedder_similar_texts_are_close() -> Result<()> {
        let e = HashEmbedder::new(256);
        let v = e
            .embed(&["de kat zit op de mat".into(), "de kat zit op een mat".into(), "beurskoersen dalen sterk".into()])
            .await?;
        let dot = |a: &Vec<f32>, b: &Vec<f32>| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        assert!(dot(&v[0], &v[1]) > dot(&v[0], &v[2]));
        Ok(())
    }
}
