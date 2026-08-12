//! Encode and decode payloads with the [bytewords](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-012-bytewords.md) scheme.
//!
//! # Standard style
//! ```
//! use bcur::bytewords::{Style, decode, encode};
//! let data = b"Some bytes";
//! let encoded = encode(data, Style::Standard);
//! assert_eq!(
//!     encoded,
//!     "guru jowl join inch crux iced kick jury inch junk taxi aqua kite limp"
//! );
//! assert_eq!(data.as_slice(), decode(&encoded, Style::Standard).unwrap());
//! ```

use alloc::{string::String, vec::Vec};

use crate::constants::BYTES_INDEXED_BY_HASH;
/// Minimal two-letter (first+last) table used by UR bodies.
pub use crate::constants::MINIMALS;
/// BCR-2020-012 four-letter bytewords table.
pub use crate::constants::WORDS;
use crate::crc32;
use crate::{Error, Result};

/// The three bytewords encoding styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Style {
    /// Four-letter words separated by spaces.
    Standard,
    /// Four-letter words separated by dashes.
    Uri,
    /// Two-letter (first + last) concatenated with no separator. Used by UR bodies.
    Minimal,
}

/// Encodes `data` with a trailing CRC-32 (ISO-HDLC) as bytewords.
///
/// Empty `data` is allowed (checksum-only body).
#[must_use]
pub fn encode(data: &[u8], style: Style) -> String {
    let checksum = crc32::checksum(data).to_be_bytes();
    encode_words(data.iter().chain(checksum.iter()).copied(), style)
}

/// Encodes `data` **without** a CRC trailer.
///
/// **Not for UR bodies.** Intended for short human identifiers only.
#[must_use]
pub fn encode_raw(data: &[u8], style: Style) -> String {
    encode_words(data.iter().copied(), style)
}

fn encode_words(data: impl Iterator<Item = u8>, style: Style) -> String {
    let words: Vec<&str> = match style {
        Style::Standard | Style::Uri => data.map(word_at).collect(),
        Style::Minimal => data.map(minimal_at).collect(),
    };
    let separator = match style {
        Style::Standard => " ",
        Style::Uri => "-",
        Style::Minimal => "",
    };
    words.join(separator)
}

/// Decodes a bytewords string and verifies its CRC-32 trailer.
///
/// # Errors
///
/// Returns an error if the string is non-ASCII, contains invalid words,
/// has an invalid length for the style, or fails the checksum.
pub fn decode(encoded: &str, style: Style) -> Result<Vec<u8>> {
    if !encoded.is_ascii() {
        return Err(Error::NonAscii);
    }

    let separator = match style {
        Style::Standard => ' ',
        Style::Uri => '-',
        Style::Minimal => return decode_minimal(encoded),
    };
    decode_parts(encoded.split(separator), false)
}

fn decode_minimal(encoded: &str) -> Result<Vec<u8>> {
    if encoded.len() % 2 != 0 {
        return Err(Error::InvalidBytewordsLength);
    }
    let parts = (0..encoded.len())
        .step_by(2)
        .map(|idx| encoded.get(idx..idx + 2).unwrap_or(""));
    decode_parts(parts, true)
}

#[inline]
fn word_at(b: u8) -> &'static str {
    // Full 256-entry table; `get` is infallible for any `u8`.
    WORDS.get(usize::from(b)).copied().unwrap_or("")
}

#[inline]
fn minimal_at(b: u8) -> &'static str {
    MINIMALS.get(usize::from(b)).copied().unwrap_or("")
}

fn encoded_byte(part: &str, minimal: bool) -> Option<u8> {
    let bytes = part.as_bytes();
    let expected_len = if minimal { 2 } else { 4 };
    if bytes.len() != expected_len {
        return None;
    }
    let first = *bytes.first()?;
    let last = *bytes.get(expected_len - 1)?;
    let hash = usize::try_from((25 * u32::from(first) + 11 * u32::from(last)) % 628).ok()?;
    let byte = BYTES_INDEXED_BY_HASH.get(hash).copied().flatten()?;
    let expected = if minimal {
        minimal_at(byte)
    } else {
        word_at(byte)
    };
    (part == expected).then_some(byte)
}

fn decode_parts<'a, I>(parts: I, minimal: bool) -> Result<Vec<u8>>
where
    I: Iterator<Item = &'a str>,
{
    let data: Vec<u8> = parts
        .map(|part| encoded_byte(part, minimal))
        .collect::<Option<Vec<_>>>()
        .ok_or(Error::InvalidWord)?;
    strip_checksum(data)
}

fn strip_checksum(mut data: Vec<u8>) -> Result<Vec<u8>> {
    if data.len() < 4 {
        return Err(Error::InvalidBytewordsChecksum);
    }
    let split = data.len() - 4;
    let (payload, checksum) = data.split_at(split);
    if crc32::checksum(payload).to_be_bytes() == *checksum {
        data.truncate(split);
        Ok(data)
    } else {
        Err(Error::InvalidBytewordsChecksum)
    }
}

/// Canonicalizes a 2–4 letter token to the full 4-letter lowercase byteword.
#[must_use]
pub fn canonicalize_byteword(token: &str) -> Option<String> {
    if !token.is_ascii() {
        return None;
    }
    let lower = token.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    match bytes.len() {
        4 => {
            let byte = encoded_byte(&lower, false)?;
            Some(String::from(word_at(byte)))
        }
        2 => {
            let byte = encoded_byte(&lower, true)?;
            Some(String::from(word_at(byte)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytewords() {
        let input = vec![0, 1, 2, 128, 255];
        assert_eq!(
            encode(&input, Style::Standard),
            "able acid also lava zoom jade need echo taxi"
        );
        assert_eq!(
            encode(&input, Style::Uri),
            "able-acid-also-lava-zoom-jade-need-echo-taxi"
        );
        assert_eq!(encode(&input, Style::Minimal), "aeadaolazmjendeoti");

        assert_eq!(
            decode(
                "able acid also lava zoom jade need echo taxi",
                Style::Standard
            )
            .unwrap(),
            input
        );
        assert_eq!(
            decode("able-acid-also-lava-zoom-jade-need-echo-taxi", Style::Uri).unwrap(),
            input
        );
        assert_eq!(decode("aeadaolazmjendeoti", Style::Minimal).unwrap(), input);

        decode(&encode(&[], Style::Minimal), Style::Minimal).unwrap();

        assert_eq!(
            decode(
                "able acid also lava zero jade need echo wolf",
                Style::Standard
            )
            .unwrap_err(),
            Error::InvalidBytewordsChecksum
        );
        assert_eq!(
            decode("axxe tied also webs lung", Style::Standard).unwrap_err(),
            Error::InvalidWord
        );
        assert_eq!(
            decode("aea", Style::Minimal).unwrap_err(),
            Error::InvalidBytewordsLength
        );
        assert_eq!(decode("₿", Style::Standard).unwrap_err(), Error::NonAscii);
    }

    #[test]
    fn test_encoding_long() {
        let input: [u8; 100] = [
            245, 215, 20, 198, 241, 235, 69, 59, 209, 205, 165, 18, 150, 158, 116, 135, 229, 212,
            19, 159, 17, 37, 239, 240, 253, 11, 109, 191, 37, 242, 38, 120, 223, 41, 156, 189, 242,
            254, 147, 204, 66, 163, 216, 175, 191, 72, 169, 54, 32, 60, 144, 230, 210, 137, 184,
            197, 33, 113, 88, 14, 157, 31, 177, 46, 1, 115, 205, 69, 225, 150, 65, 235, 58, 144,
            65, 240, 133, 69, 113, 247, 63, 53, 242, 165, 160, 144, 26, 13, 79, 237, 133, 71, 82,
            69, 254, 165, 138, 41, 85, 24,
        ];
        let encoded = "yank toys bulb skew when warm free fair tent swan \
                       open brag mint noon jury list view tiny brew note \
                       body data webs what zinc bald join runs data whiz \
                       days keys user diet news ruby whiz zone menu surf \
                       flew omit trip pose runs fund part even crux fern \
                       math visa tied loud redo silk curl jugs hard beta \
                       next cost puma drum acid junk swan free very mint \
                       flap warm fact math flap what limp free jugs yell \
                       fish epic whiz open numb math city belt glow wave \
                       limp fuel grim free zone open love diet gyro cats \
                       fizz holy city puff";
        let encoded_minimal = "yktsbbswwnwmfefrttsnonbgmtnnjyltvwtybwne\
                               bydawswtzcbdjnrsdawzdsksurdtnsrywzzemusf\
                               fwottppersfdptencxfnmhvatdldroskcljshdba\
                               ntctpadmadjksnfevymtfpwmftmhfpwtlpfejsyl\
                               fhecwzonnbmhcybtgwwelpflgmfezeonledtgocs\
                               fzhycypf";
        assert_eq!(decode(encoded, Style::Standard).unwrap(), input.to_vec());
        assert_eq!(
            decode(encoded_minimal, Style::Minimal).unwrap(),
            input.to_vec()
        );
        assert_eq!(encode(&input, Style::Standard), encoded);
        assert_eq!(encode(&input, Style::Minimal), encoded_minimal);
    }

    #[test]
    fn test_single_zero() {
        assert_eq!(encode(&[0], Style::Minimal), "aetdaowslg");
        assert_eq!(decode("aetdaowslg", Style::Minimal).unwrap(), vec![0]);
    }
}
