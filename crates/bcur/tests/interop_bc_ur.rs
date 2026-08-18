#![allow(
    unused_crate_dependencies,
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::excessive_nesting,
    reason = "integration targets link full dev-deps and host unwraps by design"
)]

//! Integration checks aligned with bc-ur public examples (wire bytes only).
//!
//! Does not copy bc-ur source; only well-known published UR strings / CBOR.

use bcur::{Decoder, Encoder, Kind, UrType, decode, encode, qr_string};

#[test]
fn bc_ur_array_123_single_part() {
    let cbor = hex::decode("83010203").unwrap();
    let ur = encode(&cbor, &UrType::new("test").unwrap());
    assert_eq!(ur, "ur:test/lsadaoaxjygonesw");
    let (kind, data) = decode(&ur).unwrap();
    assert_eq!(kind, Kind::SinglePart);
    assert_eq!(data, cbor);
}

#[test]
fn uppercase_qr_roundtrip_single_and_multi() {
    let cbor = hex::decode("83010203").unwrap();
    let lower = encode(&cbor, &UrType::new("test").unwrap());
    let upper = qr_string(&lower);
    assert_eq!(decode(&upper).unwrap(), decode(&lower).unwrap());

    let data = b"bc-ur multipath".repeat(8);
    let mut encoder = Encoder::bytes(&data, 6).unwrap();
    let mut decoder = Decoder::default();
    while !decoder.complete() {
        let part = encoder.next_part().unwrap();
        decoder.receive(&qr_string(&part)).unwrap();
    }
    assert_eq!(decoder.message().unwrap().as_deref(), Some(data.as_slice()));
}
