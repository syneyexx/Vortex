//! Hete paden van het geheugen (W06.13): vectorzoeken (int8, 512 dims) en kwantiseren.
//! Draaien: `cargo bench -p vortex-memory`.
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use vortex_memory::vector::{FlatI8Index, VectorIndex, normalize, quantize};
use vortex_types::MemoryId;

fn vecs(n: usize, d: usize) -> Vec<Vec<f32>> {
    let mut x: u64 = 7;
    (0..n)
        .map(|_| {
            normalize(
                (0..d)
                    .map(|_| {
                        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                        ((x >> 33) as f32 / u32::MAX as f32) - 0.25
                    })
                    .collect(),
                d,
            )
        })
        .collect()
}

fn bench(c: &mut Criterion) {
    let data = vecs(100_000, 512);
    let idx = FlatI8Index::new(512);
    for v in &data {
        let (q, s) = quantize(v);
        idx.upsert(MemoryId::new(), q, s);
    }
    let q = data[123].clone();
    c.bench_function("vector_search_100k_512d_top10", |b| b.iter(|| black_box(idx.search(&q, 10, &|_| true))));
    c.bench_function("quantize_512d", |b| b.iter(|| black_box(quantize(&q))));
}

criterion_group! { name = benches; config = Criterion::default().sample_size(20); targets = bench }
criterion_main!(benches);
