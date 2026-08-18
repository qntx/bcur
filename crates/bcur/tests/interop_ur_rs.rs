#![allow(
    unused_crate_dependencies,
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::excessive_nesting,
    reason = "integration targets link full dev-deps and host unwraps by design"
)]

//! Integration interop checks against ur-rs 0.5 public vectors.
//!
//! Vector tables originate from ur-rs 0.5.x (MIT); see repository-root
//! `THIRD_PARTY.md` and `tests/vectors/` (goldens only; no `URKit` / `bc-ur`
//! source).

use std::collections::BTreeSet;

use bcur::{Decoder, Encoder, Kind, UrType, decode, encode};

fn testdata_lines(raw: &str) -> Vec<&str> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

fn multipart_20() -> Vec<&'static str> {
    testdata_lines(include_str!("vectors/ur_rs_multipart_20.txt"))
}

fn published_singles() -> Vec<&'static str> {
    testdata_lines(include_str!("vectors/published_single.txt"))
}

fn published_from_refs() -> Vec<&'static str> {
    testdata_lines(include_str!("vectors/published_from_refs.txt"))
}

fn published_single_containing(needle: &str) -> &'static str {
    published_singles()
        .into_iter()
        .find(|uri| uri.contains(needle))
        .expect("published_single.txt missing expected UR")
}

#[test]
fn single_part_wolf_uri_matches_ur_rs() {
    let golden = published_single_containing("hdeymejtswhh");
    let (kind, payload) = decode(golden).unwrap();
    assert_eq!(kind, Kind::SinglePart);
    assert_eq!(encode(&payload, &UrType::bytes()), golden);
}

#[test]
fn multipart_encoder_matches_ur_rs_first_and_last() {
    // Recover the Wolf/256 CBOR-bytes payload by decoding the nine simple parts.
    let uris = multipart_20();
    let mut decoder = Decoder::default();
    for uri in uris.iter().take(9) {
        decoder.receive(uri).unwrap();
    }
    assert!(decoder.complete());
    let payload = decoder.message().unwrap().unwrap();

    let mut encoder = Encoder::bytes(&payload, 30).unwrap();
    assert_eq!(encoder.fragment_count(), 9);
    let first = *uris.first().expect("20-URI table");
    let last = *uris.last().expect("20-URI table");
    assert_eq!(encoder.next_part().unwrap(), first);
    for _ in 0..18 {
        let _ = encoder.next_part().unwrap();
    }
    assert_eq!(encoder.next_part().unwrap(), last);
}

#[test]
fn crypto_request_single_part_matches_ur_rs() {
    let mut e = minicbor::Encoder::new(Vec::new());
    let uuid = hex::decode("020C223A86F7464693FC650EF3CAC047").unwrap();
    let seed_digest =
        hex::decode("E824467CAFFEAF3BBC3E0CA095E660A9BAD80DDB6A919433A37161908B9A3986").unwrap();
    e.map(2)
        .unwrap()
        .u8(1)
        .unwrap()
        .tag(minicbor::data::Tag::new(37))
        .unwrap()
        .bytes(&uuid)
        .unwrap()
        .u8(2)
        .unwrap()
        .tag(minicbor::data::Tag::new(500))
        .unwrap()
        .map(1)
        .unwrap()
        .u8(1)
        .unwrap()
        .tag(minicbor::data::Tag::new(600))
        .unwrap()
        .bytes(&seed_digest)
        .unwrap();
    let data = e.into_writer();

    let encoded = encode(&data, &UrType::new("crypto-request").unwrap());
    assert_eq!(
        encoded,
        "ur:crypto-request/oeadtpdagdaobncpftlnylfgfgmuztihbawfsgrtflaotaadwkoyadtaaohdhdcxvsdkfgkepezepefrrffmbnnbmdvahnptrdtpbtuyimmemweootjshsmhlunyeslnameyhsdi"
    );
    assert_eq!(decode(&encoded).unwrap(), (Kind::SinglePart, data));
}

#[test]
fn multipart_roundtrip_lossy_channel() {
    let data = b"Ten chars!".repeat(20);
    let mut encoder = Encoder::bytes(&data, 8).unwrap();
    let mut decoder = Decoder::default();
    while !decoder.complete() {
        let part = encoder.next_part().unwrap();
        if encoder.current_index() & 1 != 0 {
            decoder.receive(&part).unwrap();
        }
    }
    assert_eq!(decoder.message().unwrap().as_deref(), Some(data.as_slice()));
}

/// `published_from_refs.txt` is the weekly exact-set allowlist (full extract).
/// It must contain the 20-URI table and every in-tree published single that
/// the pinned files actually quote.
#[test]
fn published_from_refs_contains_multipart_and_in_tree_singles() {
    let refs: BTreeSet<&str> = published_from_refs().into_iter().collect();
    let multipart = multipart_20();
    assert_eq!(multipart.len(), 20, "ur_rs_multipart_20.txt");
    for uri in &multipart {
        assert!(
            refs.contains(uri),
            "published_from_refs.txt missing multipart {uri}"
        );
    }

    let singles = published_singles();
    assert!(
        singles.contains(&"ur:test/lsadaoaxjygonesw"),
        "published_single.txt missing lsadaoaxjygonesw"
    );
    assert!(
        singles.contains(&"ur:bytes/iehsjyhspmwfwfia"),
        "published_single.txt missing iehsjyhspmwfwfia"
    );
    let wolf = published_single_containing("hdeymejtswhh");
    assert!(
        singles.contains(&wolf),
        "published_single.txt missing wolf 50-byte"
    );
    // Only the Wolf/50 single is a quoted literal in the pinned URKit / bc-ur
    // files. The other two are BCR / docs examples; do not union them into
    // the allowlist or the weekly exact-set diff goes red.
    assert!(
        refs.contains(wolf),
        "published_from_refs.txt missing wolf 50-byte {wolf}"
    );

    let raw = published_from_refs();
    let sorted_unique = raw.iter().zip(raw.iter().skip(1)).all(|(a, b)| a < b);
    assert!(
        sorted_unique,
        "published_from_refs.txt must be sorted unique"
    );
}
