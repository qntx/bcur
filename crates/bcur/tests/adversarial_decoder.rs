//! Adversarial multi-part decoder session behavior (public API).

#![allow(
    unused_crate_dependencies,
    clippy::tests_outside_test_module,
    reason = "integration test crates inherit package deps; not library API surface"
)]

use bcur::{Decoder, DecoderLimits, Encoder, Error, UrType};

#[test]
fn uri_len_limit_poisons_session() {
    let data = b"Ten chars!".repeat(8);
    let mut enc = Encoder::bytes(&data, 5).expect("encoder");
    let part = enc.next_part().expect("part");

    let mut decoder = Decoder::with_limits(DecoderLimits {
        max_uri_len: 16,
        ..DecoderLimits::default()
    });
    assert!(matches!(
        decoder.receive(&part),
        Err(Error::ResourceLimit("uri_len"))
    ));
    assert!(decoder.is_poisoned());
    assert!(matches!(
        decoder.receive(&part),
        Err(Error::ResourceLimit("uri_len"))
    ));
    assert!(matches!(
        decoder.message(),
        Err(Error::ResourceLimit("uri_len"))
    ));
}

#[test]
fn fragment_count_limit_poisons() {
    let data = b"Ten chars!".repeat(16);
    let mut enc = Encoder::bytes(&data, 4).expect("encoder");
    assert!(enc.fragment_count() > 1);

    let mut decoder = Decoder::with_limits(DecoderLimits {
        max_fragment_count: 1,
        ..DecoderLimits::default()
    });
    assert!(matches!(
        decoder.receive(&enc.next_part().expect("part")),
        Err(Error::ResourceLimit("fragment_count"))
    ));
    assert!(decoder.is_poisoned());
}

#[test]
fn message_length_limit_poisons() {
    let data = b"Ten chars!".repeat(16);
    let mut enc = Encoder::bytes(&data, 8).expect("encoder");
    let mut decoder = Decoder::with_limits(DecoderLimits {
        max_message_length: 8,
        ..DecoderLimits::default()
    });
    assert!(matches!(
        decoder.receive(&enc.next_part().expect("part")),
        Err(Error::ResourceLimit("message_length"))
    ));
    assert!(decoder.is_poisoned());
}

#[test]
fn type_stickiness_does_not_poison() {
    let data = b"Ten chars!".repeat(6);
    let mut a = Encoder::new(&data, 5, &UrType::new("alpha").expect("t")).expect("a");
    let mut b = Encoder::new(&data, 5, &UrType::new("beta").expect("t")).expect("b");
    let mut decoder = Decoder::default();
    decoder.receive(&a.next_part().expect("a1")).expect("recv");
    assert!(matches!(
        decoder.receive(&b.next_part().expect("b1")),
        Err(Error::UnexpectedType { .. })
    ));
    assert!(!decoder.is_poisoned());
    decoder.receive(&a.next_part().expect("a2")).expect("recv");
}

#[test]
fn not_multipart_does_not_poison() {
    let mut decoder = Decoder::default();
    assert!(matches!(
        decoder.receive("ur:bytes/iehsjyhspmwfwfia"),
        Err(Error::NotMultiPart)
    ));
    assert!(!decoder.is_poisoned());
}

#[test]
fn expected_type_mismatch_does_not_poison() {
    let data = b"Ten chars!".repeat(4);
    let mut enc = Encoder::new(&data, 5, &UrType::new("alpha").expect("t")).expect("enc");
    let mut decoder = Decoder::default().with_expected_type(UrType::new("beta").expect("t"));
    assert!(matches!(
        decoder.receive(&enc.next_part().expect("part")),
        Err(Error::UnexpectedType { .. })
    ));
    assert!(!decoder.is_poisoned());
}

#[test]
fn index_path_mismatch_does_not_poison() {
    let data = b"Ten chars!".repeat(4);
    let mut enc = Encoder::bytes(&data, 5).expect("enc");
    let part = enc.next_part().expect("part");
    let corrupted = part.replacen("/1-", "/2-", 1);
    let mut decoder = Decoder::default();
    assert!(matches!(
        decoder.receive(&corrupted),
        Err(Error::InvalidIndices)
    ));
    assert!(!decoder.is_poisoned());
}
