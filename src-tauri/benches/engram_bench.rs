// Engram Benchmarks — Criterion Suite
//
// Benchmarks for the HNSW vector index, context builder assembly,
// and BM25 search paths. Run with:
//
//   cargo bench --bench engram_bench
//
// Reports are generated in target/criterion/

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

// ═══════════════════════════════════════════════════════════════════════════
// HNSW Vector Index
// ═══════════════════════════════════════════════════════════════════════════

fn random_vec(dims: usize) -> Vec<f32> {
    (0..dims).map(|_| rand::random::<f32>() - 0.5).collect()
}

fn bench_hnsw_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("hnsw_insert");

    for &count in &[100, 1_000, 5_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &count| {
            b.iter(|| {
                let mut index = paw_temp_lib::engine::engram::hnsw::HnswIndex::new();
                for i in 0..count {
                    index.insert(&format!("mem-{}", i), random_vec(384));
                }
                black_box(index.len())
            });
        });
    }

    group.finish();
}

fn bench_hnsw_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("hnsw_search");

    for &count in &[100, 1_000, 5_000] {
        // Build index once
        let mut index = paw_temp_lib::engine::engram::hnsw::HnswIndex::new();
        for i in 0..count {
            index.insert(&format!("mem-{}", i), random_vec(384));
        }

        let query = random_vec(384);

        group.bench_with_input(
            BenchmarkId::new("k10", count),
            &(index, query),
            |b, (index, query)| {
                b.iter(|| {
                    let results = index.search(black_box(query), 10, 0.0);
                    black_box(results.len())
                });
            },
        );
    }

    group.finish();
}

fn bench_hnsw_brute_force_comparison(c: &mut Criterion) {
    let dims = 384;
    let count = 1_000;

    // Build HNSW index
    let mut hnsw = paw_temp_lib::engine::engram::hnsw::HnswIndex::new();
    let mut vecs: Vec<Vec<f32>> = Vec::with_capacity(count);
    for i in 0..count {
        let v = random_vec(dims);
        hnsw.insert(&format!("mem-{}", i), v.clone());
        vecs.push(v);
    }

    let query = random_vec(dims);

    let mut group = c.benchmark_group("vector_search_1k");

    group.bench_function("hnsw", |b| {
        b.iter(|| {
            let results = hnsw.search(black_box(&query), 10, 0.0);
            black_box(results.len())
        });
    });

    group.bench_function("brute_force", |b| {
        b.iter(|| {
            let mut scored: Vec<(usize, f64)> = vecs
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let sim = cosine_sim(black_box(&query), v);
                    (i, sim)
                })
                .collect();
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(10);
            black_box(scored.len())
        });
    });

    group.finish();
}

#[inline]
fn cosine_sim(a: &[f32], b: &[f32]) -> f64 {
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b.iter()) {
        let (xf, yf) = (*x as f64, *y as f64);
        dot += xf * yf;
        na += xf * xf;
        nb += yf * yf;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom < 1e-12 {
        0.0
    } else {
        dot / denom
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Groups
// ═══════════════════════════════════════════════════════════════════════════

criterion_group!(
    benches,
    bench_hnsw_insert,
    bench_hnsw_search,
    bench_hnsw_brute_force_comparison,
);
criterion_main!(benches);
