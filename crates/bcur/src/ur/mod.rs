//! Uniform Resource encode/decode and multi-part fountain transport.
//!
//! ```
//! let data = b"Ten chars!".repeat(10);
//! let mut encoder = bcur::Encoder::bytes(&data, 5).unwrap();
//! let mut decoder = bcur::Decoder::default();
//! while !decoder.complete() {
//!     let part = encoder.next_part().unwrap();
//!     if encoder.current_index() & 1 > 0 {
//!         decoder.receive(&part).unwrap();
//!     }
//! }
//! assert_eq!(decoder.message().unwrap().as_deref(), Some(data.as_slice()));
//! ```

use alloc::{string::String, vec::Vec};

use crate::bytewords::{self, Style};
use crate::fountain::{self, DecoderLimits};
use crate::{Error, Result};

/// Validated UR type token (non-empty, stored lowercase).
///
/// Allowed characters after normalization: ASCII `[a-z0-9-]+`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UrType(String);

impl UrType {
    /// Validates and lowercases a type token.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidType`] if empty or containing illegal characters.
    pub fn new(s: &str) -> Result<Self> {
        let lower = s.to_ascii_lowercase();
        validate_type(&lower)?;
        Ok(Self(lower))
    }

    /// Returns the string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Well-known `bytes` type.
    #[must_use]
    pub fn bytes() -> Self {
        Self(String::from("bytes"))
    }
}

impl TryFrom<&str> for UrType {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

/// Whether a decoded UR is single- or multi-part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Full payload in one URI.
    SinglePart,
    /// One fountain part of a multi-part stream.
    MultiPart,
}

/// Owned parse of a UR string (body is case-folded to lowercase).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUr {
    /// Normalized type.
    pub ur_type: UrType,
    /// Single- or multi-part.
    pub kind: Kind,
    /// Multi-part path indices `(seq, count)`, if multi-part.
    pub indices: Option<(u32, u32)>,
    /// Lowercased bytewords body.
    pub body: String,
}

/// Lowercases a UR string for case-insensitive QR transport.
#[must_use]
pub fn normalize_ur(uri: &str) -> String {
    uri.to_ascii_lowercase()
}

/// Encodes a single-part UR. Empty `data` is allowed.
///
/// # Errors
///
/// Only fails if `ur_type` is invalid (already-validated types always succeed).
pub fn encode(data: &[u8], ur_type: &UrType) -> Result<String> {
    let body = bytewords::encode(data, Style::Minimal);
    Ok(alloc::format!("ur:{}/{body}", ur_type.as_str()))
}

/// Decodes payload bytes from a single- or multi-part UR (type is discarded).
///
/// Multi-part returns the CBOR-encoded fountain part bytes, not the message.
///
/// # Errors
///
/// Returns parse, type, bytewords, or index errors.
pub fn decode(uri: &str) -> Result<(Kind, Vec<u8>)> {
    let (kind, payload, _) = decode_with_indices(uri)?;
    Ok((kind, payload))
}

/// Like [`decode`] but retains the normalized type.
///
/// # Errors
///
/// Same as [`decode`].
pub fn decode_with_type(uri: &str) -> Result<(UrType, Kind, Vec<u8>)> {
    let parsed = parse(uri)?;
    let payload = bytewords::decode(&parsed.body, Style::Minimal)?;
    Ok((parsed.ur_type, parsed.kind, payload))
}

/// Parses a UR into an owned structure (full-URI case fold).
///
/// # Errors
///
/// Returns scheme, type, or index errors. Does not decode bytewords.
pub fn parse(uri: &str) -> Result<ParsedUr> {
    let lowered = normalize_ur(uri);
    parse_normalized(&lowered)
}

/// Parses an already-lowercase UR without re-allocating a case-fold buffer.
///
/// # Errors
///
/// Returns scheme, type, or index errors.
pub fn parse_normalized(uri: &str) -> Result<ParsedUr> {
    let strip_scheme = uri.strip_prefix("ur:").ok_or(Error::InvalidScheme)?;
    let (type_str, rest) = strip_scheme
        .split_once('/')
        .ok_or(Error::TypeUnspecified)?;
    let ur_type = UrType::new(type_str)?;

    match rest.rsplit_once('/') {
        None => Ok(ParsedUr {
            ur_type,
            kind: Kind::SinglePart,
            indices: None,
            body: String::from(rest),
        }),
        Some((indices, body)) => {
            let indices = decode_indices(indices)?;
            Ok(ParsedUr {
                ur_type,
                kind: Kind::MultiPart,
                indices: Some(indices),
                body: String::from(body),
            })
        }
    }
}

fn decode_with_indices(value: &str) -> Result<(Kind, Vec<u8>, Option<(u32, u32)>)> {
    let parsed = parse(value)?;
    let payload = bytewords::decode(&parsed.body, Style::Minimal)?;
    Ok((parsed.kind, payload, parsed.indices))
}

fn decode_indices(indices: &str) -> Result<(u32, u32)> {
    let (idx, idx_total) = indices.split_once('-').ok_or(Error::InvalidIndices)?;
    let idx = idx.parse::<u32>().map_err(|_| Error::InvalidIndices)?;
    let idx_total = idx_total
        .parse::<u32>()
        .map_err(|_| Error::InvalidIndices)?;
    if idx == 0 || idx_total == 0 {
        return Err(Error::InvalidIndices);
    }
    Ok((idx, idx_total))
}

fn validate_type(s: &str) -> Result<()> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err(Error::InvalidType);
    }
    Ok(())
}

/// Multi-part UR encoder owning the payload.
#[derive(Debug)]
pub struct Encoder {
    fountain: fountain::Encoder,
    ur_type: UrType,
}

impl Encoder {
    /// Creates an encoder with the well-known `bytes` type.
    ///
    /// # Errors
    ///
    /// Propagates fountain construction errors.
    pub fn bytes(message: &[u8], max_fragment_length: usize) -> Result<Self> {
        Self::new(message, max_fragment_length, &UrType::bytes())
    }

    /// Creates an encoder with a custom type.
    ///
    /// # Errors
    ///
    /// Propagates type validation and fountain construction errors.
    pub fn new(message: &[u8], max_fragment_length: usize, ur_type: &UrType) -> Result<Self> {
        Ok(Self {
            fountain: fountain::Encoder::new(message, max_fragment_length)?,
            ur_type: ur_type.clone(),
        })
    }

    /// Emits the next multi-part UR string.
    ///
    /// # Errors
    ///
    /// Returns sequence resource limits from the fountain encoder.
    pub fn next_part(&mut self) -> Result<String> {
        let part = self.fountain.next_part()?;
        let body = bytewords::encode(&part.to_cbor(), Style::Minimal);
        Ok(alloc::format!(
            "ur:{}/{}/{body}",
            self.ur_type.as_str(),
            part.sequence_id()
        ))
    }

    /// Current emitted part count.
    #[must_use]
    pub const fn current_index(&self) -> u32 {
        self.fountain.current_sequence()
    }

    /// Source fragment count `K`.
    #[must_use]
    pub fn fragment_count(&self) -> u32 {
        self.fountain.fragment_count()
    }
}

/// Multi-part UR decoder.
#[derive(Debug)]
pub struct Decoder {
    fountain: fountain::Decoder,
    max_uri_len: usize,
    expected_type: Option<UrType>,
    seen_type: Option<UrType>,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    /// Creates a decoder with default limits.
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(DecoderLimits::default())
    }

    /// Creates a decoder with custom fountain/URI limits.
    #[must_use]
    pub fn with_limits(limits: DecoderLimits) -> Self {
        Self {
            fountain: fountain::Decoder::with_limits(limits),
            max_uri_len: limits.max_uri_len,
            expected_type: None,
            seen_type: None,
        }
    }

    /// Requires every received part to match `ur_type`.
    #[must_use]
    pub fn with_expected_type(mut self, ur_type: UrType) -> Self {
        self.expected_type = Some(ur_type);
        self
    }

    /// Receives one multi-part UR string.
    ///
    /// # Errors
    ///
    /// Returns parse, type, index, bytewords, CBOR, or fountain errors.
    pub fn receive(&mut self, value: &str) -> Result<()> {
        if value.len() > self.max_uri_len {
            return Err(Error::ResourceLimit("uri_len"));
        }

        let parsed = parse(value)?;
        if parsed.kind != Kind::MultiPart {
            return Err(Error::NotMultiPart);
        }

        if let Some(ref expected) = self.expected_type {
            if &parsed.ur_type != expected {
                return Err(Error::UnexpectedType {
                    expected: String::from(expected.as_str()),
                    found: String::from(parsed.ur_type.as_str()),
                });
            }
        }

        match &self.seen_type {
            None => self.seen_type = Some(parsed.ur_type.clone()),
            Some(seen) if seen != &parsed.ur_type => {
                return Err(Error::UnexpectedType {
                    expected: String::from(seen.as_str()),
                    found: String::from(parsed.ur_type.as_str()),
                });
            }
            Some(_) => {}
        }

        let decoded = bytewords::decode(&parsed.body, Style::Minimal)?;
        let part = fountain::Part::from_cbor(decoded.as_slice())?;
        let (idx, idx_total) = parsed.indices.ok_or(Error::InvalidIndices)?;
        if part.sequence() != idx || part.sequence_count() != idx_total {
            return Err(Error::InvalidIndices);
        }
        self.fountain.receive(part)?;
        Ok(())
    }

    /// Whether the message is fully recovered.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.fountain.complete()
    }

    /// Returns the decoded message if complete.
    ///
    /// # Errors
    ///
    /// Propagates fountain message errors.
    pub fn message(&self) -> Result<Option<Vec<u8>>> {
        self.fountain.message()
    }

    /// Resolved source fragment count, or `None` before any part.
    #[must_use]
    pub fn resolved_fragment_count(&self) -> Option<usize> {
        self.fountain.resolved_fragment_count()
    }

    /// Total fragment count `K` (0 before any part).
    #[must_use]
    pub fn fragment_count(&self) -> usize {
        self.fountain.fragment_count()
    }

    /// Whether the underlying fountain decoder is poisoned.
    #[must_use]
    pub const fn is_poisoned(&self) -> bool {
        self.fountain.is_poisoned()
    }
}

/// Uppercase UR string for QR efficiency.
#[must_use]
pub fn qr_string(ur: &str) -> String {
    ur.to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::test_utils::make_message;
    use minicbor::bytes::ByteVec;

    fn make_message_ur(length: usize, seed: &str) -> Vec<u8> {
        let message = make_message(seed, length);
        minicbor::to_vec(ByteVec::from(message)).unwrap()
    }

    #[test]
    fn test_single_part_ur() {
        let ur = make_message_ur(50, "Wolf");
        let encoded = encode(&ur, &UrType::bytes()).unwrap();
        let expected = "ur:bytes/hdeymejtswhhylkepmykhhtsytsnoyoyaxaedsuttydmmhhpktpmsrjtgwdpfnsboxgwlbaawzuefywkdplrsrjynbvygabwjldapfcsdwkbrkch";
        assert_eq!(encoded, expected);
        let decoded = decode(&encoded).unwrap();
        assert_eq!((Kind::SinglePart, ur), decoded);
    }

    #[test]
    fn test_ur_encoder() {
        let ur = make_message_ur(256, "Wolf");
        let mut encoder = Encoder::bytes(&ur, 30).unwrap();
        let expected = [
            "ur:bytes/1-9/lpadascfadaxcywenbpljkhdcahkadaemejtswhhylkepmykhhtsytsnoyoyaxaedsuttydmmhhpktpmsrjtdkgslpgh",
            "ur:bytes/2-9/lpaoascfadaxcywenbpljkhdcagwdpfnsboxgwlbaawzuefywkdplrsrjynbvygabwjldapfcsgmghhkhstlrdcxaefz",
            "ur:bytes/3-9/lpaxascfadaxcywenbpljkhdcahelbknlkuejnbadmssfhfrdpsbiegecpasvssovlgeykssjykklronvsjksopdzmol",
        ];
        assert_eq!(encoder.fragment_count(), 9);
        for (index, e) in expected.into_iter().enumerate() {
            assert_eq!(encoder.current_index() as usize, index);
            assert_eq!(encoder.next_part().unwrap(), e);
        }
    }

    #[test]
    fn test_multipart_ur() {
        let ur = make_message_ur(32767, "Wolf");
        let mut encoder = Encoder::bytes(&ur, 1000).unwrap();
        let mut decoder = Decoder::default();
        while !decoder.complete() {
            assert_eq!(decoder.message().unwrap(), None);
            decoder.receive(&encoder.next_part().unwrap()).unwrap();
        }
        assert_eq!(decoder.message().unwrap(), Some(ur));
    }

    #[test]
    fn test_data_encode() {
        assert_eq!(
            encode(b"data", &UrType::bytes()).unwrap(),
            "ur:bytes/iehsjyhspmwfwfia"
        );
    }

    #[test]
    fn test_case_fold() {
        let lower = encode(b"data", &UrType::bytes()).unwrap();
        let upper = qr_string(&lower);
        assert_eq!(decode(&upper).unwrap(), decode(&lower).unwrap());
    }

    #[test]
    fn test_type_stickiness() {
        let data = b"Ten chars!".repeat(5);
        let mut enc_a = Encoder::new(&data, 5, &UrType::new("alpha").unwrap()).unwrap();
        let mut enc_b = Encoder::new(&data, 5, &UrType::new("beta").unwrap()).unwrap();
        let mut decoder = Decoder::default();
        decoder.receive(&enc_a.next_part().unwrap()).unwrap();
        assert!(matches!(
            decoder.receive(&enc_b.next_part().unwrap()),
            Err(Error::UnexpectedType { .. })
        ));
    }

    #[test]
    fn test_invalid_scheme() {
        assert!(matches!(
            decode("uhr:bytes/aeadaolazmjendeoti"),
            Err(Error::InvalidScheme)
        ));
    }

    #[test]
    fn test_custom_encoder() {
        let data = b"Ten chars!";
        let mut encoder =
            Encoder::new(data, 5, &UrType::new("my-scheme").unwrap()).unwrap();
        assert_eq!(
            encoder.next_part().unwrap(),
            "ur:my-scheme/1-2/lpadaobkcywkwmhfwnfeghihjtcxiansvomopr"
        );
    }

    #[test]
    fn test_not_multipart() {
        let mut decoder = Decoder::default();
        assert!(matches!(
            decoder.receive("ur:bytes/iehsjyhspmwfwfia"),
            Err(Error::NotMultiPart)
        ));
    }

    #[test]
    fn test_bc_ur_example() {
        // CBOR array [1, 2, 3] as single-part ur:test (bc-ur golden)
        // We only check roundtrip of raw CBOR bytes through bytewords path
        // using the known string from bc-ur docs when payload is correct CBOR.
        let cbor = hex::decode("83010203").unwrap(); // array(3) [1,2,3]
        let ur = encode(&cbor, &UrType::new("test").unwrap()).unwrap();
        assert_eq!(ur, "ur:test/lsadaoaxjygonesw");
        let (kind, data) = decode(&ur).unwrap();
        assert_eq!(kind, Kind::SinglePart);
        assert_eq!(data, cbor);
    }
}
