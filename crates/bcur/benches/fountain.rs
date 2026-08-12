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

//! Fountain encode/decode throughput.

use bcur::fountain::{Decoder, Encoder};
use criterion::{Criterion, criterion_group, criterion_main};

fn bench_fountain(c: &mut Criterion) {
    let message: Vec<u8> = (0..256).map(|i| (i % 256) as u8).collect();

    c.bench_function("fountain_encode_9_parts", |b| {
        b.iter(|| {
            let mut enc = Encoder::new(&message, 30).unwrap();
            for _ in 0..9 {
                let _ = enc.next_part().unwrap();
            }
        });
    });

    c.bench_function("fountain_roundtrip_256", |b| {
        b.iter(|| {
            let mut enc = Encoder::new(&message, 30).unwrap();
            let mut dec = Decoder::default();
            while !dec.complete() {
                dec.receive(enc.next_part().unwrap()).unwrap();
            }
            let _ = dec.message().unwrap();
        });
    });
}

criterion_group!(benches, bench_fountain);
criterion_main!(benches);
