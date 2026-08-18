#![no_main]

use bcur::{Decoder, DecoderLimits, Error};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data);
    let mut decoder = Decoder::with_limits(DecoderLimits::default());
    match decoder.receive(&s) {
        Err(Error::ResourceLimit(kind)) => {
            assert!(
                decoder.is_poisoned(),
                "ResourceLimit({kind:?}) must poison the session"
            );
            match decoder.receive(&s) {
                Err(Error::ResourceLimit(again)) if again == kind => {}
                other => panic!("poisoned receive must replay {kind:?}, got {other:?}"),
            }
            match decoder.message() {
                Err(Error::ResourceLimit(again)) if again == kind => {}
                other => panic!("poisoned message must replay {kind:?}, got {other:?}"),
            }
        }
        Ok(()) => {
            if decoder.complete() {
                match decoder.message() {
                    Ok(Some(_)) => {}
                    Err(
                        Error::InvalidPadding | Error::InvalidMessageChecksum | Error::DecoderState,
                    ) => {}
                    other => {
                        panic!("complete() message must be Some or a join error, got {other:?}")
                    }
                }
            }
            if let Some(ty) = decoder.ur_type() {
                let other = if ty.as_str() == "zzzz" {
                    "yyyy"
                } else {
                    "zzzz"
                };
                let probe = format!("ur:{other}/iehsjyhspmwfwfia");
                let poisoned = decoder.is_poisoned();
                match decoder.receive(&probe) {
                    Err(Error::UnexpectedType { .. }) => {
                        assert_eq!(
                            decoder.is_poisoned(),
                            poisoned,
                            "type pin is UnexpectedType, not poison"
                        );
                    }
                    other => panic!("pinned type must reject {other:?}"),
                }
            }
        }
        Err(_) => {}
    }
});
