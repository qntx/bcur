//! Adversarial multi-part decoder session behavior (public API).

use crate::{Decoder, DecoderLimits, Encoder, Error, UrType};

#[test]
fn uri_len_limit_poisons_session() {
    let data = b"Ten chars!".repeat(8);
    let mut enc = Encoder::bytes(&data, 5).expect("encoder");
    let part = enc.next_part().expect("part");

    let mut decoder = Decoder::with_limits(DecoderLimits {
        max_uri_len: 16,
        ..DecoderLimits::default()
    });
    assert!(
        matches!(decoder.receive(&part), Err(Error::ResourceLimit("uri_len"))),
        "oversized URI must be ResourceLimit"
    );
    assert!(decoder.is_poisoned(), "session must be poisoned");
    assert!(
        matches!(decoder.receive(&part), Err(Error::ResourceLimit("uri_len"))),
        "poisoned session stays fail-closed"
    );
    assert!(
        matches!(decoder.message(), Err(Error::ResourceLimit("uri_len"))),
        "message() stays fail-closed"
    );
}

#[test]
fn fragment_count_limit_poisons() {
    let data = b"Ten chars!".repeat(16);
    let mut enc = Encoder::bytes(&data, 4).expect("encoder");
    assert!(enc.fragment_count() > 1, "need K > 1");

    let mut decoder = Decoder::with_limits(DecoderLimits {
        max_fragment_count: 1,
        ..DecoderLimits::default()
    });
    let part = enc.next_part().expect("part");
    assert!(
        matches!(
            decoder.receive(&part),
            Err(Error::ResourceLimit("fragment_count"))
        ),
        "fragment_count limit"
    );
    assert!(decoder.is_poisoned(), "poisoned after fragment_count");
}

#[test]
fn message_length_limit_poisons() {
    let data = b"Ten chars!".repeat(16);
    let mut enc = Encoder::bytes(&data, 8).expect("encoder");
    let mut decoder = Decoder::with_limits(DecoderLimits {
        max_message_length: 8,
        ..DecoderLimits::default()
    });
    assert!(
        matches!(
            decoder.receive(&enc.next_part().expect("part")),
            Err(Error::ResourceLimit("message_length"))
        ),
        "message_length limit"
    );
    assert!(decoder.is_poisoned(), "poisoned after message_length");
}

#[test]
fn type_stickiness_does_not_poison() {
    let data = b"Ten chars!".repeat(6);
    let mut a = Encoder::new(&data, 5, &UrType::new("alpha").expect("type")).expect("encoder a");
    let mut b = Encoder::new(&data, 5, &UrType::new("beta").expect("type")).expect("encoder b");
    let mut decoder = Decoder::default();
    decoder
        .receive(&a.next_part().expect("a1"))
        .expect("recv a");
    assert!(
        matches!(
            decoder.receive(&b.next_part().expect("b1")),
            Err(Error::UnexpectedType { .. })
        ),
        "type mismatch"
    );
    assert!(!decoder.is_poisoned(), "type errors must not poison");
    decoder
        .receive(&a.next_part().expect("a2"))
        .expect("recv a2");
}

#[test]
fn not_multipart_does_not_poison() {
    let mut decoder = Decoder::default();
    assert!(
        matches!(
            decoder.receive("ur:bytes/iehsjyhspmwfwfia"),
            Err(Error::NotMultiPart)
        ),
        "single-part rejected"
    );
    assert!(!decoder.is_poisoned(), "not multi-part must not poison");
}

#[test]
fn expected_type_mismatch_does_not_poison() {
    let data = b"Ten chars!".repeat(4);
    let mut enc = Encoder::new(&data, 5, &UrType::new("alpha").expect("type")).expect("encoder");
    let mut decoder = Decoder::default().with_expected_type(UrType::new("beta").expect("type"));
    assert!(
        matches!(
            decoder.receive(&enc.next_part().expect("part")),
            Err(Error::UnexpectedType { .. })
        ),
        "expected type mismatch"
    );
    assert!(
        !decoder.is_poisoned(),
        "expected-type error must not poison"
    );
}

#[test]
fn index_path_mismatch_does_not_poison() {
    let data = b"Ten chars!".repeat(4);
    let mut enc = Encoder::bytes(&data, 5).expect("encoder");
    let part = enc.next_part().expect("part");
    let corrupted = part.replacen("/1-", "/2-", 1);
    let mut decoder = Decoder::default();
    assert!(
        matches!(decoder.receive(&corrupted), Err(Error::InvalidIndices)),
        "path/index mismatch"
    );
    assert!(!decoder.is_poisoned(), "index errors must not poison");
}
