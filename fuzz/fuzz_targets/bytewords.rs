#![no_main]

use bcur::Error;
use bcur::bytewords::{self, Style};
use libfuzzer_sys::fuzz_target;

const STYLES: [Style; 3] = [Style::Standard, Style::Uri, Style::Minimal];

fn style_from_tag(tag: u8) -> Style {
    match tag % 3 {
        0 => Style::Standard,
        1 => Style::Uri,
        _ => Style::Minimal,
    }
}

fuzz_target!(|data: &[u8]| {
    let (tag, payload) = match data.split_first() {
        Some(split) => split,
        None => return,
    };
    let tagged = style_from_tag(*tag);

    for style in STYLES {
        let encoded = bytewords::encode(payload, style);
        let decoded = bytewords::decode(&encoded, style).expect("encode/decode roundtrip");
        assert_eq!(
            decoded, payload,
            "decode(encode(x, {style:?})) must equal x"
        );
    }

    let garbage = String::from_utf8_lossy(data);
    if !garbage.is_ascii() {
        assert_eq!(
            bytewords::decode(&garbage, tagged),
            Err(Error::NonAscii),
            "non-ASCII must be NonAscii"
        );
    } else {
        let _ = bytewords::decode(&garbage, tagged);
    }
});
