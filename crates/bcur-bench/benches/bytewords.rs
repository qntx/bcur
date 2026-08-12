//! Bytewords encode/decode throughput.
//!
//! `criterion_group!` / `criterion_main!` expand to items without rustdoc;
//! that is a criterion limitation, not missing public API documentation.

#![allow(
    missing_docs,
    reason = "criterion macros expand to undocumented harness items"
)]

use bcur::bytewords::{Style, decode, encode};
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

fn payloads() -> [(usize, Vec<u8>); 3] {
    [
        (32, (0_u8..32).collect()),
        (256, (0_u8..=255).collect()),
        (
            4096,
            (0..4096)
                .map(|i| u8::try_from(i % 256).unwrap_or(0))
                .collect(),
        ),
    ]
}

fn bench_bytewords(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytewords");
    for (n, data) in payloads() {
        group.bench_with_input(BenchmarkId::new("encode_minimal", n), &data, |b, data| {
            b.iter(|| encode(black_box(data), Style::Minimal));
        });
        let encoded = encode(&data, Style::Minimal);
        group.bench_with_input(BenchmarkId::new("decode_minimal", n), &encoded, |b, s| {
            b.iter(|| black_box(decode(black_box(s), Style::Minimal)));
        });
        group.bench_with_input(BenchmarkId::new("encode_standard", n), &data, |b, data| {
            b.iter(|| encode(black_box(data), Style::Standard));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_bytewords);
criterion_main!(benches);
