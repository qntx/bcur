#![no_main]

use bcur::fountain::Part;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(part) = Part::from_cbor_with_max(data, 8192, 2000) {
        let again = Part::from_cbor(&part.to_cbor()).expect("shortest-form part must re-decode");
        assert_eq!(part, again, "from_cbor(to_cbor(p)) must equal p");
    }
});
