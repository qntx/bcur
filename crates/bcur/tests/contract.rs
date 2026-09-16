#![allow(
    unused_crate_dependencies,
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::excessive_nesting,
    reason = "integration targets link full dev-deps and host unwraps by design"
)]

//! Canonical contract vectors for ur.js + bcur sister interop.

use serde_json::Value;

use bcur::bytewords::{self, Style};
use bcur::fountain::{self, Part};
use bcur::{Decoder, DecoderLimits, Encoder, Error, Kind, ResourceKind, UrType, decode, encode};

fn json(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap()
}

fn json_str<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap()
}

fn json_u32(v: &Value, key: &str) -> u32 {
    u32::try_from(v.get(key).and_then(Value::as_u64).unwrap()).unwrap()
}

fn json_usize(v: &Value, key: &str) -> usize {
    usize::try_from(v.get(key).and_then(Value::as_u64).unwrap()).unwrap()
}

fn data_lines(raw: &str) -> Vec<&str> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

fn assert_line_file(raw: &str) {
    assert!(raw.ends_with('\n'), "line files need a trailing newline");
    assert!(!raw.contains('\r'), "line files are LF-only");
    assert!(
        !raw.lines().any(|line| line.starts_with('#')),
        "line files have no header comments"
    );
}

fn assert_session_poison(kind: ResourceKind, decoder: &mut Decoder, part: &str) {
    assert!(
        matches!(
            decoder.receive(part),
            Err(Error::ResourceLimit(k)) if k == kind
        ),
        "first receive must be ResourceLimit({kind:?})"
    );
    assert!(decoder.is_poisoned(), "resource limit poisons the session");
    assert!(
        matches!(
            decoder.receive(part),
            Err(Error::ResourceLimit(k)) if k == kind
        ),
        "later receive must repeat ResourceLimit({kind:?})"
    );
    assert!(
        matches!(
            decoder.message(),
            Err(Error::ResourceLimit(k)) if k == kind
        ),
        "message must repeat ResourceLimit({kind:?})"
    );
}

#[test]
fn readme_is_canonical_paragraph() {
    let readme = include_str!("vectors/contract/README.md");
    assert!(readme.contains("ur.js"));
    assert!(readme.contains("bcur"));
    assert!(readme.contains("implementation bug"));
    assert!(readme.contains("THIRD_PARTY.md"));
    assert!(readme.contains("data-only"));
}

#[test]
fn bytewords_contract() {
    let spec = json(include_str!("vectors/contract/bytewords.json"));
    let input = hex::decode(json_str(&spec, "inputHex")).unwrap();
    assert_eq!(
        bytewords::encode(&input, Style::Standard),
        json_str(&spec, "standard")
    );
    assert_eq!(
        bytewords::encode(&input, Style::Uri),
        json_str(&spec, "uri")
    );
    assert_eq!(
        bytewords::encode(&input, Style::Minimal),
        json_str(&spec, "minimal")
    );
    assert_eq!(
        bytewords::decode(json_str(&spec, "standard"), Style::Standard).unwrap(),
        input
    );
    assert_eq!(
        bytewords::decode(json_str(&spec, "uri"), Style::Uri).unwrap(),
        input
    );
    assert_eq!(
        bytewords::decode(json_str(&spec, "minimal"), Style::Minimal).unwrap(),
        input
    );
}

#[test]
fn part_cbor_contract() {
    let spec = json(include_str!("vectors/contract/part-cbor.json"));
    let cbor = hex::decode(json_str(&spec, "cborHex")).unwrap();
    let part = Part::from_cbor(&cbor).unwrap();
    assert_eq!(part.sequence(), json_u32(&spec, "sequence"));
    assert_eq!(part.sequence_count(), json_u32(&spec, "sequenceCount"));
    assert_eq!(part.message_length(), json_u32(&spec, "messageLength"));
    assert_eq!(part.checksum(), json_u32(&spec, "checksum"));
    assert_eq!(hex::encode(part.data()), json_str(&spec, "dataHex"));
    assert_eq!(hex::encode(part.to_cbor()), json_str(&spec, "cborHex"));

    let non_shortest = hex::decode(json_str(&spec, "nonShortestSequenceCborHex")).unwrap();
    assert!(matches!(
        Part::from_cbor(&non_shortest),
        Err(Error::InvalidPartCbor)
    ));
}

#[test]
fn k1_contract() {
    let spec = json(include_str!("vectors/contract/k1.json"));
    let payload = json_str(&spec, "payloadUtf8").as_bytes();
    let ur_type = UrType::new(json_str(&spec, "type")).unwrap();
    let mut encoder = Encoder::new(payload, 64, &ur_type).unwrap();
    assert!(encoder.is_single_part());
    let outbound = encoder.next_part().unwrap();
    assert!(!outbound.contains(json_str(&spec, "outboundMustNotContain")));
    assert_eq!(
        spec.get("outboundEqualsSinglePartEncode")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(outbound, encode(payload, &ur_type));
    assert_eq!(
        spec.get("inboundFountain11Accepted")
            .and_then(Value::as_bool),
        Some(true)
    );
    let mut fountain = fountain::Encoder::new(payload, 64).unwrap();
    let part = fountain.next_part().unwrap();
    let body = bytewords::encode(&part.to_cbor(), Style::Minimal);
    let uri = format!("ur:{}/1-1/{body}", ur_type.as_str());
    let mut decoder = Decoder::default();
    decoder.receive(&uri).unwrap();
    assert!(decoder.complete());
    assert_eq!(decoder.message().unwrap().as_deref(), Some(payload));
}

#[test]
fn l4_test_array_contract() {
    let spec = json(include_str!("vectors/contract/l4-test-array.json"));
    let cbor = hex::decode(json_str(&spec, "cborHex")).unwrap();
    let ur_type = UrType::new(json_str(&spec, "type")).unwrap();
    let uri = json_str(&spec, "uri");
    assert_eq!(encode(&cbor, &ur_type), uri);

    #[cfg(feature = "dcbor")]
    {
        let ur = bcur::Ur::new("test", vec![1, 2, 3]).unwrap();
        assert_eq!(ur.string(), uri);
    }

    let (kind, data) = decode(json_str(&spec, "uriUpper")).unwrap();
    assert_eq!(kind, Kind::SinglePart);
    assert_eq!(data, cbor);
}

#[test]
fn decoder_limits_contract() {
    let spec = json(include_str!("vectors/contract/decoder-limits.json"));
    let limits = DecoderLimits::default();
    assert_eq!(
        limits.max_message_length,
        json_usize(&spec, "maxMessageLength")
    );
    assert_eq!(
        limits.max_fragment_count,
        json_usize(&spec, "maxFragmentCount")
    );
    assert_eq!(
        limits.max_fragment_data_length,
        json_usize(&spec, "maxFragmentDataLength")
    );
    assert_eq!(limits.max_buffer_parts, json_usize(&spec, "maxBufferParts"));
    assert_eq!(
        limits.max_received_parts,
        json_usize(&spec, "maxReceivedParts")
    );
    assert_eq!(limits.max_uri_len, json_usize(&spec, "maxUriLen"));
}

#[test]
fn poison_maps_via_rust_ident() {
    let spec = json(include_str!("vectors/contract/poison.json"));
    let raw = include_str!("vectors/contract/poison.json");
    assert!(!raw.contains("DecoderState"));
    let rows = spec.get("limits").and_then(Value::as_array).unwrap();
    let kinds = [
        ResourceKind::UriLen,
        ResourceKind::FragmentCount,
        ResourceKind::FragmentData,
        ResourceKind::MessageLength,
        ResourceKind::ReceivedParts,
        ResourceKind::BufferParts,
        ResourceKind::Sequence,
    ];
    assert_eq!(rows.len(), kinds.len());
    for (row, kind) in rows.iter().zip(kinds) {
        assert_eq!(json_str(row, "rust"), format!("{kind:?}"));
        let session = row.get("sessionPoison").and_then(Value::as_bool).unwrap();
        // Sequence is encoder-only; `next_sequence` is crate-private (fountain unit tests).
        if kind == ResourceKind::Sequence {
            assert!(!session);
        } else {
            assert!(session);
        }
    }
}

#[test]
fn poison_receive_and_message_same_code() {
    let spec = json(include_str!("vectors/contract/poison.json"));
    let names: Vec<&str> = spec
        .get("receiveAndMessageSameCode")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(names, ["uri_len", "fragment_count"]);

    let uri_payload = b"Ten chars!".repeat(8);
    let mut uri_enc = Encoder::bytes(&uri_payload, 5).unwrap();
    let uri_part = uri_enc.next_part().unwrap();
    let mut uri_decoder = Decoder::with_limits(DecoderLimits {
        max_uri_len: 16,
        ..DecoderLimits::default()
    });
    assert_session_poison(ResourceKind::UriLen, &mut uri_decoder, &uri_part);

    let fragment_payload = b"Ten chars!".repeat(16);
    let mut fragment_enc = Encoder::bytes(&fragment_payload, 4).unwrap();
    assert!(fragment_enc.fragment_count() > 1);
    let mut fragment_decoder = Decoder::with_limits(DecoderLimits {
        max_fragment_count: 1,
        ..DecoderLimits::default()
    });
    let fragment_part = fragment_enc.next_part().unwrap();
    assert_session_poison(
        ResourceKind::FragmentCount,
        &mut fragment_decoder,
        &fragment_part,
    );
}

#[test]
fn poison_not_poison_errors() {
    let spec = json(include_str!("vectors/contract/poison.json"));
    let names = spec
        .get("notPoison")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(names.contains(&"UnexpectedType"));
    assert!(names.contains(&"CborDecode"));

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
    decoder.receive(&a.next_part().unwrap()).unwrap();

    #[cfg(feature = "dcbor")]
    {
        use bcur::{CborErrorKind, MultipartDecoder};

        let mut typed = MultipartDecoder::new();
        typed.receive("ur:bytes/iehsjyhspmwfwfia").unwrap();
        assert!(typed.complete());
        assert!(matches!(
            typed.message(),
            Err(Error::Cbor(ref c)) if c.kind() == CborErrorKind::Decode
        ));
        assert!(!typed.is_poisoned());
    }
}

#[test]
fn fountain_mixed_contract() {
    let mixed = include_str!("vectors/contract/fountain-mixed.txt");
    assert_line_file(mixed);
    let uris = data_lines(mixed);
    assert_eq!(uris.len(), 20, "fountain-mixed.txt");
    assert_eq!(
        uris,
        data_lines(include_str!("vectors/ur_rs_multipart_20.txt"))
    );

    let mut decoder = Decoder::default();
    for uri in &uris {
        decoder.receive(uri).unwrap();
    }
    assert!(decoder.complete());
    let payload = decoder.message().unwrap().unwrap();
    let mut encoder = Encoder::bytes(&payload, 30).unwrap();
    assert_eq!(encoder.fragment_count(), 9);
    assert_eq!(
        encoder.next_part().unwrap(),
        *uris.first().expect("20-URI table")
    );
}

#[test]
fn published_singles_contract() {
    let raw = include_str!("vectors/contract/published-singles.txt");
    assert_line_file(raw);
    let uris = data_lines(raw);
    assert_eq!(uris.len(), 3, "published-singles.txt");
    assert_eq!(
        uris,
        data_lines(include_str!("vectors/published_single.txt"))
    );
    for uri in uris {
        let (kind, _) = decode(uri).unwrap();
        assert_eq!(kind, Kind::SinglePart);
    }
}
