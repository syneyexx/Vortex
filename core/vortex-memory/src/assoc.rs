//! Neural memory module (W18.01): differentieerbaar associatief geheugen (modern Hopfield).
//! Score(q, kᵢ) = softmax(β · (W q) · kᵢ) met een geleerde projectie W (start = identiteit),
//! online getraind op (vraag, relevante herinnering)-paren (gebruik, 👍) met cross-entropy.
//! Als `Reranker` inzetbaar; aan alleen bij gemeten winst op de retrieval-eval (gate W18).
// VX:W18.01

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::embed::Embedder;
use crate::error::Result;
use crate::retrieve::Reranker;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssocMemory {
    pub dims: usize,
    /// Rij-major d×d.
    pub w: Vec<f32>,
    pub beta: f32,
    pub lr: f32,
    pub updates: u64,
}

fn norm(v: &[f32]) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
    v.iter().map(|x| x / n).collect()
}

impl AssocMemory {
    pub fn new(dims: usize) -> Self {
        let mut w = vec![0.0; dims * dims];
        for i in 0..dims {
            w[i * dims + i] = 1.0;
        }
        Self { dims, w, beta: 8.0, lr: 0.5, updates: 0 }
    }

    fn project(&self, q: &[f32]) -> Vec<f32> {
        let d = self.dims;
        let q = norm(q);
        norm(&(0..d).map(|i| (0..d).map(|j| self.w[i * d + j] * q[j]).sum()).collect::<Vec<f32>>())
    }

    /// Hopfield-aandacht over de sleutels (kansen, som 1).
    pub fn attend(&self, q: &[f32], keys: &[Vec<f32>]) -> Vec<f32> {
        let p = self.project(q);
        let logits: Vec<f32> =
            keys.iter().map(|k| self.beta * norm(k).iter().zip(&p).map(|(a, b)| a * b).sum::<f32>()).collect();
        let m = logits.iter().cloned().fold(f32::MIN, f32::max);
        let e: Vec<f32> = logits.iter().map(|l| (l - m).exp()).collect();
        let s = e.iter().sum::<f32>().max(1e-12);
        e.iter().map(|x| x / s).collect()
    }

    /// Eén gradiëntstap: maximaliseer log p(pos | q) tegen de negatieven.
    /// dL/dW ≈ β (Σ pᵢ kᵢ − k_pos) qᵀ (projectie-normalisatie genegeerd: kleine stappen).
    pub fn train(&mut self, q: &[f32], pos: &[f32], negs: &[Vec<f32>]) -> f32 {
        let mut keys = vec![pos.to_vec()];
        keys.extend(negs.iter().cloned());
        let p = self.attend(q, &keys);
        let ks: Vec<Vec<f32>> = keys.iter().map(|k| norm(k)).collect();
        let qn = norm(q);
        let d = self.dims;
        let mut g = vec![0.0f32; d];
        for (i, k) in ks.iter().enumerate() {
            let coef = p[i] - if i == 0 { 1.0 } else { 0.0 };
            for (gj, kj) in g.iter_mut().zip(k) {
                *gj += coef * kj;
            }
        }
        for (i, row) in self.w.chunks_mut(d).enumerate() {
            let gi = self.beta * g[i] * self.lr;
            if gi == 0.0 {
                continue;
            }
            for (wij, qj) in row.iter_mut().zip(&qn) {
                *wij -= gi * qj;
            }
        }
        self.updates += 1;
        -(p[0].max(1e-9)).ln()
    }
}

/// Reranker op basis van het associatieve geheugen.
#[derive(Debug, Clone)]
pub struct AssocReranker {
    pub mem: Arc<RwLock<AssocMemory>>,
    pub embedder: Arc<dyn Embedder>,
}

#[async_trait::async_trait]
impl Reranker for AssocReranker {
    async fn rerank(&self, query: &str, docs: &[String]) -> Result<Vec<f32>> {
        let mut texts = vec![query.to_string()];
        texts.extend(docs.iter().cloned());
        let v = self.embedder.embed(&texts).await?;
        let (q, keys) = v.split_first().map(|(a, b)| (a.clone(), b.to_vec())).unwrap_or_default();
        Ok(self.mem.read().attend(&q, &keys))
    }
}

impl AssocReranker {
    /// Trainen op paren (vraag, relevante tekst) met de overige teksten als negatieven.
    pub async fn fit(&self, pairs: &[(String, String)], pool: &[String], epochs: usize) -> Result<f32> {
        let mut qs = Vec::new();
        for (q, _) in pairs {
            qs.push(q.clone());
        }
        let qv = self.embedder.embed(&qs).await?;
        let pv = self.embedder.embed(pool).await?;
        let mut loss = 0.0;
        for _ in 0..epochs {
            loss = 0.0;
            for (i, (_, pos)) in pairs.iter().enumerate() {
                let Some(pi) = pool.iter().position(|t| t == pos) else { continue };
                let negs: Vec<Vec<f32>> =
                    pv.iter().enumerate().filter(|(j, _)| *j != pi).map(|(_, v)| v.clone()).collect();
                loss += self.mem.write().train(&qv[i], &pv[pi], &negs);
            }
        }
        Ok(loss / pairs.len().max(1) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mrr(m: &AssocMemory, qs: &[Vec<f32>], keys: &[Vec<f32>]) -> f32 {
        qs.iter()
            .enumerate()
            .map(|(i, q)| {
                let p = m.attend(q, keys);
                1.0 / (1 + p.iter().filter(|x| **x > p[i]).count()) as f32
            })
            .sum::<f32>()
            / qs.len() as f32
    }

    #[test]
    fn learns_association_beyond_cosine() {
        // Sleutels willekeurig; vragen = vaste "vertaling" (permutatie + tekenwissel) van de
        // sleutel + ruis: cosinus faalt, het geleerde geheugen leert de vertaling.
        let d = 16;
        let mut s: u64 = 7;
        let mut rnd = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s % 10_000) as f32 / 5_000.0 - 1.0
        };
        let keys: Vec<Vec<f32>> = (0..40).map(|_| (0..d).map(|_| rnd()).collect()).collect();
        let tr = |k: &Vec<f32>, r: &mut dyn FnMut() -> f32| -> Vec<f32> {
            (0..d).map(|i| -k[(i + 5) % d] + 0.1 * r()).collect()
        };
        let qs: Vec<Vec<f32>> = keys.iter().map(|k| tr(k, &mut rnd)).collect();
        let mut m = AssocMemory::new(d);
        let before = mrr(&m, &qs[20..], &keys[20..]);
        for _ in 0..60 {
            for i in 0..20 {
                let negs: Vec<Vec<f32>> = (0..20).filter(|j| *j != i).map(|j| keys[j].clone()).collect();
                m.train(&qs[i], &keys[i], &negs);
            }
        }
        let after = mrr(&m, &qs[20..], &keys[20..]);
        assert!(after > before + 0.3, "voor {before:.2} na {after:.2} (op niet-getrainde paren)");
    }
}
