//! Integration checks aligned with published bc-ur wire examples.

#![allow(
    unused_crate_dependencies,
    clippy::tests_outside_test_module,
    reason = "integration test crates inherit package deps; not library API surface"
)]

use bcur::{Decoder, Encoder, Kind, UrType, decode, encode, qr_string};

#[test]
fn bc_ur_array_123_single_part() {
    let cbor = hex::decode("83010203").expect("hex");
    let ur = encode(&cbor, &UrType::new("test").expect("type")).expect("encode");
    assert_eq!(ur, "ur:test/lsadaoaxjygonesw");
    let (kind, data) = decode(&ur).expect("decode");
    assert_eq!(kind, Kind::SinglePart);
    assert_eq!(data, cbor);
}

#[test]
fn uppercase_qr_roundtrip_single_and_multi() {
    let cbor = hex::decode("83010203").expect("hex");
    let lower = encode(&cbor, &UrType::new("test").expect("type")).expect("encode");
    let upper = qr_string(&lower);
    assert_eq!(decode(&upper).expect("up"), decode(&lower).expect("lo"));

    let data = b"bc-ur multipath".repeat(8);
    let mut encoder = Encoder::bytes(&data, 6).expect("encoder");
    let mut decoder = Decoder::default();
    while !decoder.complete() {
        let part = encoder.next_part().expect("part");
        decoder.receive(&qr_string(&part)).expect("receive");
    }
    assert_eq!(
        decoder.message().expect("message").as_deref(),
        Some(data.as_slice())
    );
}
