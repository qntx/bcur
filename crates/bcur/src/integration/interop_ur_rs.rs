//! Interop checks against ur-rs 0.5 public vectors (MIT; see `THIRD_PARTY.md`).

use crate::{Decoder, Encoder, Kind, UrType, decode, encode};

#[test]
fn single_part_wolf_uri_matches_ur_rs() {
    let golden = "ur:bytes/hdeymejtswhhylkepmykhhtsytsnoyoyaxaedsuttydmmhhpktpmsrjtgwdpfnsboxgwlbaawzuefywkdplrsrjynbvygabwjldapfcsdwkbrkch";
    let (kind, payload) = decode(golden).expect("decode golden");
    assert_eq!(kind, Kind::SinglePart, "expected single-part UR");
    assert_eq!(
        encode(&payload, &UrType::bytes()).expect("re-encode"),
        golden,
        "re-encode must be byte-identical"
    );
}

#[test]
fn multipart_encoder_matches_ur_rs_first_and_last() {
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
        decoder.receive(uri).expect("receive simple part");
    }
    assert!(decoder.complete(), "nine simples should complete K=9");
    let payload = decoder
        .message()
        .expect("message ok")
        .expect("message present");

    let mut encoder = Encoder::bytes(&payload, 30).expect("encoder");
    assert_eq!(encoder.fragment_count(), 9, "fragment count");
    assert_eq!(
        encoder.next_part().expect("part 1"),
        "ur:bytes/1-9/lpadascfadaxcywenbpljkhdcahkadaemejtswhhylkepmykhhtsytsnoyoyaxaedsuttydmmhhpktpmsrjtdkgslpgh"
    );
    for _ in 0..18 {
        let _ = encoder.next_part().expect("middle part");
    }
    assert_eq!(
        encoder.next_part().expect("part 20"),
        "ur:bytes/20-9/lpbbascfadaxcywenbpljkhdcayapmrleeleaxpasfrtrdkncffwjyjzgyetdmlewtkpktgllepfrltataztksmhkbot"
    );
}

#[test]
fn crypto_request_single_part_matches_ur_rs() {
    let mut e = minicbor::Encoder::new(Vec::new());
    let uuid = hex::decode("020C223A86F7464693FC650EF3CAC047").expect("uuid hex");
    let seed_digest =
        hex::decode("E824467CAFFEAF3BBC3E0CA095E660A9BAD80DDB6A919433A37161908B9A3986")
            .expect("digest hex");
    e.map(2)
        .and_then(|enc| enc.u8(1))
        .and_then(|enc| enc.tag(minicbor::data::Tag::new(37)))
        .and_then(|enc| enc.bytes(&uuid))
        .and_then(|enc| enc.u8(2))
        .and_then(|enc| enc.tag(minicbor::data::Tag::new(500)))
        .and_then(|enc| enc.map(1))
        .and_then(|enc| enc.u8(1))
        .and_then(|enc| enc.tag(minicbor::data::Tag::new(600)))
        .and_then(|enc| enc.bytes(&seed_digest))
        .expect("encode crypto-request CBOR");
    let data = e.into_writer();

    let encoded = encode(&data, &UrType::new("crypto-request").expect("type")).expect("encode UR");
    assert_eq!(
        encoded,
        "ur:crypto-request/oeadtpdagdaobncpftlnylfgfgmuztihbawfsgrtflaotaadwkoyadtaaohdhdcxvsdkfgkepezepefrrffmbnnbmdvahnptrdtpbtuyimmemweootjshsmhlunyeslnameyhsdi"
    );
    assert_eq!(
        decode(&encoded).expect("decode UR"),
        (Kind::SinglePart, data)
    );
}

#[test]
fn multipart_roundtrip_lossy_channel() {
    let data = b"Ten chars!".repeat(20);
    let mut encoder = Encoder::bytes(&data, 8).expect("encoder");
    let mut decoder = Decoder::default();
    while !decoder.complete() {
        let part = encoder.next_part().expect("part");
        if encoder.current_index() & 1 != 0 {
            decoder.receive(&part).expect("receive");
        }
    }
    assert_eq!(
        decoder.message().expect("message").as_deref(),
        Some(data.as_slice())
    );
}
