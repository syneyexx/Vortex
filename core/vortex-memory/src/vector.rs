//! Vectorindex achter een trait. Eerste backend: int8 "flat" in RAM, opgeslagen in SQLite
//! (zelfde transactie als de herinnering). Exact zoeken; ~1 mln × 512 dims ≈ 0,5 GB RAM.
//! ANN/LanceDB kan later achter dezelfde trait (W12, TB-schaal).
// VX:W05.02

use std::collections::HashMap;

use parking_lot::RwLock;
use vortex_types::MemoryId;

/// Kwantiseer een (genormaliseerde) vector naar int8 + schaal.
pub fn quantize(v: &[f32]) -> (Vec<i8>, f32) {
    let max = v.iter().fold(0f32, |m, x| m.max(x.abs()));
    if max == 0.0 {
        return (vec![0; v.len()], 0.0);
    }
    let scale = max / 127.0;
    (v.iter().map(|x| (x / scale).round().clamp(-127.0, 127.0) as i8).collect(), scale)
}

/// L2-normaliseer (en evt. inkorten tot `dims` — Matryoshka/MRL).
pub fn normalize(mut v: Vec<f32>, dims: usize) -> Vec<f32> {
    v.truncate(dims);
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in &mut v {
            *x /= n;
        }
    }
    v
}

fn dot_i8(a: &[i8], b: &[i8]) -> i32 {
    // Compiler vectoriseert deze lus (SIMD); i32-accumulatie voorkomt overloop bij ≤ 133k dims.
    a.iter().zip(b).map(|(x, y)| i32::from(*x) * i32::from(*y)).sum()
}

pub trait VectorIndex: Send + Sync + std::fmt::Debug {
    fn dims(&self) -> usize;
    fn upsert(&self, id: MemoryId, q: Vec<i8>, scale: f32);
    fn remove(&self, id: &MemoryId) -> bool;
    /// Top-k op cosinus (vectoren zijn genormaliseerd vóór kwantisatie). `allow` filtert.
    fn search(&self, query: &[f32], k: usize, allow: &dyn Fn(&MemoryId) -> bool) -> Vec<(MemoryId, f32)>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Default)]
struct Flat {
    ids: Vec<MemoryId>,
    data: Vec<i8>,
    scales: Vec<f32>,
    pos: HashMap<MemoryId, usize>,
}

/// Exacte int8-index (alles in één aaneengesloten buffer: cache-vriendelijk).
#[derive(Debug)]
pub struct FlatI8Index {
    dims: usize,
    inner: RwLock<Flat>,
}

impl FlatI8Index {
    pub fn new(dims: usize) -> Self {
        Self { dims, inner: RwLock::new(Flat::default()) }
    }
}

impl VectorIndex for FlatI8Index {
    fn dims(&self) -> usize {
        self.dims
    }

    fn upsert(&self, id: MemoryId, q: Vec<i8>, scale: f32) {
        if q.len() != self.dims {
            tracing::error!(got = q.len(), want = self.dims, "vector met verkeerde dimensie genegeerd");
            return;
        }
        let mut f = self.inner.write();
        if let Some(&i) = f.pos.get(&id) {
            f.data[i * self.dims..(i + 1) * self.dims].copy_from_slice(&q);
            f.scales[i] = scale;
            return;
        }
        let i = f.ids.len();
        f.ids.push(id);
        f.data.extend_from_slice(&q);
        f.scales.push(scale);
        f.pos.insert(id, i);
    }

    fn remove(&self, id: &MemoryId) -> bool {
        let mut f = self.inner.write();
        let Some(i) = f.pos.remove(id) else { return false };
        // Swap-remove: laatste naar het gat.
        let last = f.ids.len() - 1;
        if i != last {
            let moved = f.ids[last];
            let d = self.dims;
            let (head, tail) = f.data.split_at_mut(last * d);
            head[i * d..(i + 1) * d].copy_from_slice(&tail[..d]);
            f.ids[i] = moved;
            f.scales[i] = f.scales[last];
            f.pos.insert(moved, i);
        }
        f.ids.pop();
        f.scales.pop();
        let n = f.data.len() - self.dims;
        f.data.truncate(n);
        true
    }

    fn search(&self, query: &[f32], k: usize, allow: &dyn Fn(&MemoryId) -> bool) -> Vec<(MemoryId, f32)> {
        if query.len() != self.dims || k == 0 {
            return vec![];
        }
        let (q, qs) = quantize(query);
        let f = self.inner.read();
        let mut heap: Vec<(f32, usize)> = Vec::with_capacity(k + 1);
        for (i, chunk) in f.data.chunks_exact(self.dims).enumerate() {
            let s = dot_i8(&q, chunk) as f32 * qs * f.scales[i];
            if heap.len() < k || s > heap[heap.len() - 1].0 {
                if !allow(&f.ids[i]) {
                    continue;
                }
                let at = heap.partition_point(|(x, _)| *x >= s);
                heap.insert(at, (s, i));
                heap.truncate(k);
            }
        }
        heap.into_iter().map(|(s, i)| (f.ids[i], s)).collect()
    }

    fn len(&self) -> usize {
        self.inner.read().ids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(seed: u64, d: usize) -> Vec<f32> {
        let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        normalize(
            (0..d)
                .map(|_| {
                    x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    ((x >> 33) as f32 / u32::MAX as f32) - 0.25
                })
                .collect(),
            d,
        )
    }

    #[test]
    fn int8_cosine_close_to_float() {
        let a = v(1, 512);
        let b = v(2, 512);
        let exact: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        let (qa, sa) = quantize(&a);
        let (qb, sb) = quantize(&b);
        let approx = dot_i8(&qa, &qb) as f32 * sa * sb;
        assert!((exact - approx).abs() < 0.02, "{exact} vs {approx}");
    }

    #[test]
    fn search_finds_nearest_and_respects_filter_and_remove() {
        let idx = FlatI8Index::new(64);
        let ids: Vec<MemoryId> = (0..200).map(|_| MemoryId::new()).collect();
        for (i, id) in ids.iter().enumerate() {
            let (q, s) = quantize(&v(i as u64, 64));
            idx.upsert(*id, q, s);
        }
        let hit = idx.search(&v(42, 64), 3, &|_| true);
        assert_eq!(hit[0].0, ids[42]);
        assert!(hit[0].1 > 0.98);
        let hit = idx.search(&v(42, 64), 3, &|id| *id != ids[42]);
        assert_ne!(hit[0].0, ids[42]);
        assert!(idx.remove(&ids[42]));
        assert!(!idx.remove(&ids[42]));
        assert_eq!(idx.len(), 199);
        assert_ne!(idx.search(&v(42, 64), 1, &|_| true)[0].0, ids[42]);
        // na swap-remove nog steeds correct:
        assert_eq!(idx.search(&v(199, 64), 1, &|_| true)[0].0, ids[199]);
    }

    #[test]
    fn mrl_truncation_renormalizes() {
        let x = normalize(vec![3.0, 4.0, 100.0], 2);
        assert!((x[0] - 0.6).abs() < 1e-6 && (x[1] - 0.8).abs() < 1e-6);
    }
}
