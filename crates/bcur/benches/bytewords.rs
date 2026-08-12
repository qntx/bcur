#![allow(
    unused_crate_dependencies,
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::excessive_nesting,
    reason = "criterion benches: fixed payloads and intentional unwraps"
)]

//! Bytewords encode/decode throughput.

use bcur::bytewords::{Style, decode, encode};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

fn payloads() -> Vec<(usize, Vec<u8>)> {
    [32usize, 256, 4096]
        .into_iter()
        .map(|n| (n, (0..n).map(|i| (i % 256) as u8).collect()))
        .collect()
}

fn bench_bytewords(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytewords");
    for (n, data) in payloads() {
        group.bench_with_input(BenchmarkId::new("encode_minimal", n), &data, |b, data| {
            b.iter(|| encode(data, Style::Minimal));
        });
        let encoded = encode(&data, Style::Minimal);
        group.bench_with_input(BenchmarkId::new("decode_minimal", n), &encoded, |b, s| {
            b.iter(|| decode(s, Style::Minimal).unwrap());
        });
        group.bench_with_input(BenchmarkId::new("encode_standard", n), &data, |b, data| {
            b.iter(|| encode(data, Style::Standard));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_bytewords);
criterion_main!(benches);
