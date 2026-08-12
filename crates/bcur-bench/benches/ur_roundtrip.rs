//! Multi-part UR encode/decode throughput.
//!
//! `criterion_group!` / `criterion_main!` expand to items without rustdoc.

#![allow(
    missing_docs,
    reason = "criterion macros expand to undocumented harness items"
)]

use bcur::{Decoder, Encoder};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn ur_roundtrip(data: &[u8]) {
    let Ok(mut enc) = Encoder::bytes(data, 12) else {
        return;
    };
    let mut dec = Decoder::default();
    while !dec.complete() {
        let Ok(part) = enc.next_part() else {
            return;
        };
        drop(dec.receive(&part));
    }
    drop(dec.message());
}

fn ur_lossy(data: &[u8]) {
    let Ok(mut enc) = Encoder::bytes(data, 12) else {
        return;
    };
    let mut dec = Decoder::default();
    while !dec.complete() {
        let Ok(part) = enc.next_part() else {
            return;
        };
        if enc.current_index() & 1 != 0 {
            drop(dec.receive(&part));
        }
    }
    drop(dec.message());
}

fn bench_ur(c: &mut Criterion) {
    let data = b"Ten chars!".repeat(30);
    c.bench_function("ur_multipart_roundtrip", |b| {
        b.iter(|| ur_roundtrip(black_box(&data)));
    });
    c.bench_function("ur_multipart_lossy_skip", |b| {
        b.iter(|| ur_lossy(black_box(&data)));
    });
}

criterion_group!(benches, bench_ur);
criterion_main!(benches);
