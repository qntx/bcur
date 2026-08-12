//! Fountain encode/decode throughput.
//!
//! `criterion_group!` / `criterion_main!` expand to items without rustdoc.

#![allow(
    missing_docs,
    reason = "criterion macros expand to undocumented harness items"
)]

use bcur::fountain::{Decoder, Encoder};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn message_256() -> Vec<u8> {
    (0_u8..=255).collect()
}

fn encode_nine_parts(message: &[u8]) {
    let Ok(mut enc) = Encoder::new(message, 30) else {
        return;
    };
    for _ in 0..9 {
        drop(enc.next_part());
    }
}

fn roundtrip_256(message: &[u8]) {
    let Ok(mut enc) = Encoder::new(message, 30) else {
        return;
    };
    let mut dec = Decoder::default();
    while !dec.complete() {
        let Ok(part) = enc.next_part() else {
            return;
        };
        drop(dec.receive(part));
    }
    drop(dec.message());
}

fn bench_fountain(c: &mut Criterion) {
    let message = message_256();
    c.bench_function("fountain_encode_9_parts", |b| {
        b.iter(|| encode_nine_parts(black_box(&message)));
    });
    c.bench_function("fountain_roundtrip_256", |b| {
        b.iter(|| roundtrip_256(black_box(&message)));
    });
}

criterion_group!(benches, bench_fountain);
criterion_main!(benches);
