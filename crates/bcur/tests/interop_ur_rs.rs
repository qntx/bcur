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
//! Vector tables originate from ur-rs (MIT); see repository `THIRD_PARTY.md`.

use bcur::{Decoder, Encoder, Kind, UrType, decode, encode};

#[test]
fn single_part_wolf_uri_matches_ur_rs() {
    let golden = "ur:bytes/hdeymejtswhhylkepmykhhtsytsnoyoyaxaedsuttydmmhhpktpmsrjtgwdpfnsboxgwlbaawzuefywkdplrsrjynbvygabwjldapfcsdwkbrkch";
    let (kind, payload) = decode(golden).unwrap();
    assert_eq!(kind, Kind::SinglePart);
    assert_eq!(encode(&payload, &UrType::bytes()).unwrap(), golden);
}

#[test]
fn multipart_encoder_matches_ur_rs_first_and_last() {
    // Recover the Wolf/256 CBOR-bytes payload by decoding the nine simple parts.
    let uris = [
        "ur:bytes/1-9/lpadascfadaxcywenbpljkhdcahkadaemejtswhhylkepmykhhtsytsnoyoyaxaedsuttydmmhhpktpmsrjtdkgslpgh",
        "ur:bytes/2-9/lpaoascfadaxcywenbpljkhdcagwdpfnsboxgwlbaawzuefywkdplrsrjynbvygabwjldapfcsgmghhkhstlrdcxaefz",
        "ur:bytes/3-9/lpaxascfadaxcywenbpljkhdcahelbknlkuejnbadmssfhfrdpsbiegecpasvssovlgeykssjykklronvsjksopdzmol",
        "ur:bytes/4-9/lpaaascfadaxcywenbpljkhdcasotkhemthydawydtaxneurlkosgwcekonertkbrlwmplssjtammdplolsbrdzcrtas",
        "ur:bytes/5-9/lpahascfadaxcywenbpljkhdcatbbdfmssrkzmcwnezelennjpfzbgmuktrhtejscktelgfpdlrkfyfwdajldejokbwf",
        "ur:bytes/6-9/lpamascfadaxcywenbpljkhdcackjlhkhybssklbwefectpfnbbectrljectpavyrolkzczcpkmwidmwoxkilghdsowp",
        "ur:bytes/7-9/lpatascfadaxcywenbpljkhdcavszmwnjkwtclrtvaynhpahrtoxmwvwatmedibkaegdosftvandiodagdhthtrlnnhy",
        "ur:bytes/8-9/lpayascfadaxcywenbpljkhdcadmsponkkbbhgsoltjntegepmttmoonftnbuoiyrehfrtsabzsttorodklubbuyaetk",
        "ur:bytes/9-9/lpasascfadaxcywenbpljkhdcajskecpmdckihdyhphfotjojtfmlnwmadspaxrkytbztpbauotbgtgtaeaevtgavtny",
    ];
    let mut decoder = Decoder::default();
    for uri in uris {
        decoder.receive(uri).unwrap();
    }
    assert!(decoder.complete());
    let payload = decoder.message().unwrap().unwrap();

    let mut encoder = Encoder::bytes(&payload, 30).unwrap();
    assert_eq!(encoder.fragment_count(), 9);
    assert_eq!(
        encoder.next_part().unwrap(),
        "ur:bytes/1-9/lpadascfadaxcywenbpljkhdcahkadaemejtswhhylkepmykhhtsytsnoyoyaxaedsuttydmmhhpktpmsrjtdkgslpgh"
    );
    for _ in 0..18 {
        let _ = encoder.next_part().unwrap();
    }
    assert_eq!(
        encoder.next_part().unwrap(),
        "ur:bytes/20-9/lpbbascfadaxcywenbpljkhdcayapmrleeleaxpasfrtrdkncffwjyjzgyetdmlewtkpktgllepfrltataztksmhkbot"
    );
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

    let encoded = encode(&data, &UrType::new("crypto-request").unwrap()).unwrap();
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
