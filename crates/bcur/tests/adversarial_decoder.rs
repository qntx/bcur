#![allow(
    unused_crate_dependencies,
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::excessive_nesting,
    reason = "integration targets link full dev-deps and host unwraps by design"
)]

//! Adversarial multi-part decoder session behavior (public API).

use bcur::{Decoder, DecoderLimits, Encoder, Error, UrType};

#[test]
fn uri_len_limit_poisons_session() {
    let data = b"Ten chars!".repeat(8);
    let mut enc = Encoder::bytes(&data, 5).unwrap();
    let part = enc.next_part().unwrap();

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
    let mut enc = Encoder::bytes(&data, 4).unwrap();
    assert!(enc.fragment_count() > 1);

    let mut decoder = Decoder::with_limits(DecoderLimits {
        max_fragment_count: 1,
        ..DecoderLimits::default()
    });
    let part = enc.next_part().unwrap();
    assert!(matches!(
        decoder.receive(&part),
        Err(Error::ResourceLimit("fragment_count"))
    ));
    assert!(decoder.is_poisoned());
}

#[test]
fn message_length_limit_poisons() {
    let data = b"Ten chars!".repeat(16);
    let mut enc = Encoder::bytes(&data, 8).unwrap();
    let mut decoder = Decoder::with_limits(DecoderLimits {
        max_message_length: 8,
        ..DecoderLimits::default()
    });
    assert!(matches!(
        decoder.receive(&enc.next_part().unwrap()),
        Err(Error::ResourceLimit("message_length"))
    ));
    assert!(decoder.is_poisoned());
}

#[test]
fn type_stickiness_does_not_poison() {
    let data = b"Ten chars!".repeat(6);
    let mut a = Encoder::new(&data, 5, &UrType::new("alpha").unwrap()).unwrap();
    let mut b = Encoder::new(&data, 5, &UrType::new("beta").unwrap()).unwrap();
    let mut decoder = Decoder::default();
    decoder.receive(&a.next_part().unwrap()).unwrap();
    assert!(matches!(
        decoder.receive(&b.next_part().unwrap()),
        Err(Error::UnexpectedType { .. })
    ));
    assert!(!decoder.is_poisoned());
    // Same type continues to be accepted.
    decoder.receive(&a.next_part().unwrap()).unwrap();
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
    let mut enc = Encoder::new(&data, 5, &UrType::new("alpha").unwrap()).unwrap();
    let mut decoder = Decoder::default().with_expected_type(UrType::new("beta").unwrap());
    assert!(matches!(
        decoder.receive(&enc.next_part().unwrap()),
        Err(Error::UnexpectedType { .. })
    ));
    assert!(!decoder.is_poisoned());
}

#[test]
fn index_path_mismatch_does_not_poison() {
    let data = b"Ten chars!".repeat(4);
    let mut enc = Encoder::bytes(&data, 5).unwrap();
    let part = enc.next_part().unwrap();
    let corrupted = part.replacen("/1-", "/2-", 1);
    let mut decoder = Decoder::default();
    assert!(matches!(
        decoder.receive(&corrupted),
        Err(Error::InvalidIndices)
    ));
    assert!(!decoder.is_poisoned());
}
