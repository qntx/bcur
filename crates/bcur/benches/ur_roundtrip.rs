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

//! Multi-part UR encode/decode throughput.

use bcur::{Decoder, Encoder};
use criterion::{Criterion, criterion_group, criterion_main};

fn bench_ur(c: &mut Criterion) {
    let data = b"Ten chars!".repeat(30);

    c.bench_function("ur_multipart_roundtrip", |b| {
        b.iter(|| {
            let mut enc = Encoder::bytes(&data, 12).unwrap();
            let mut dec = Decoder::default();
            while !dec.complete() {
                dec.receive(&enc.next_part().unwrap()).unwrap();
            }
            let _ = dec.message().unwrap();
        });
    });

    c.bench_function("ur_multipart_lossy_skip", |b| {
        b.iter(|| {
            let mut enc = Encoder::bytes(&data, 12).unwrap();
            let mut dec = Decoder::default();
            while !dec.complete() {
                let part = enc.next_part().unwrap();
                if enc.current_index() & 1 != 0 {
                    dec.receive(&part).unwrap();
                }
            }
            let _ = dec.message().unwrap();
        });
    });
}

criterion_group!(benches, bench_ur);
criterion_main!(benches);
