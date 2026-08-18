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
use crate::error::Poison;
use crate::fountain::{self, DecoderLimits};
use crate::{Error, ResourceKind, Result};

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

impl TryFrom<String> for UrType {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::new(&value)
    }
}

/// Conversion into a validated [`UrType`].
///
/// Implemented for [`UrType`], `&UrType`, [`&str`], and [`String`].
/// Not for downstream impls (sealed).
pub trait IntoUrType: sealed::Sealed {
    /// Validates or clones into a [`UrType`].
    ///
    /// # Errors
    ///
    /// [`Error::InvalidType`] for an empty or illegal type token.
    fn into_ur_type(self) -> Result<UrType>;
}

#[allow(
    unreachable_pub,
    reason = "Sealed must be pub so IntoUrType can be public; module is private"
)]
mod sealed {
    use alloc::string::String;

    use super::UrType;

    /// Not implementable outside this crate.
    pub trait Sealed {}
    impl Sealed for UrType {}
    impl Sealed for &UrType {}
    impl Sealed for &str {}
    impl Sealed for String {}
}

impl IntoUrType for UrType {
    fn into_ur_type(self) -> Result<UrType> {
        Ok(self)
    }
}

impl IntoUrType for &UrType {
    fn into_ur_type(self) -> Result<UrType> {
        Ok(self.clone())
    }
}

impl IntoUrType for &str {
    fn into_ur_type(self) -> Result<UrType> {
        UrType::new(self)
    }
}

impl IntoUrType for String {
    fn into_ur_type(self) -> Result<UrType> {
        UrType::new(&self)
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
/// `ur_type` is already validated, so this cannot fail.
#[must_use]
pub fn encode(data: &[u8], ur_type: &UrType) -> String {
    let body = bytewords::encode(data, Style::Minimal);
    alloc::format!("ur:{}/{body}", ur_type.as_str())
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

/// Decodes a **single-part** UR to its payload bytes.
///
/// Multi-part URIs return [`Error::NotSinglePart`]; use [`Decoder`] for those.
///
/// # Errors
///
/// Parse, type, bytewords, or [`Error::NotSinglePart`].
pub fn decode_message(uri: &str) -> Result<Vec<u8>> {
    let (kind, payload) = decode(uri)?;
    match kind {
        Kind::SinglePart => Ok(payload),
        Kind::MultiPart => Err(Error::NotSinglePart),
    }
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
    parse_lowered(&normalize_ur(uri))
}

fn parse_lowered(uri: &str) -> Result<ParsedUr> {
    let strip_scheme = uri.strip_prefix("ur:").ok_or(Error::InvalidScheme)?;
    let (type_str, rest) = strip_scheme.split_once('/').ok_or(Error::TypeUnspecified)?;
    let ur_type = UrType::new(type_str)?;

    match rest.rsplit_once('/') {
        None => Ok(ParsedUr {
            ur_type,
            kind: Kind::SinglePart,
            indices: None,
            body: rest.to_ascii_lowercase(),
        }),
        Some((indices, body)) => {
            let indices = decode_indices(indices)?;
            Ok(ParsedUr {
                ur_type,
                kind: Kind::MultiPart,
                indices: Some(indices),
                body: body.to_ascii_lowercase(),
            })
        }
    }
}

type DecodedPayload = (Kind, Vec<u8>, Option<(u32, u32)>);

fn decode_with_indices(value: &str) -> Result<DecodedPayload> {
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

/// UR encoder owning the payload (single-part when `K == 1`).
#[derive(Debug)]
pub struct Encoder {
    fountain: fountain::Encoder,
    ur_type: UrType,
    message: Vec<u8>,
    /// 0 before first emit; 1 after. Only used when `K == 1`.
    single_emitted: u32,
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
    /// When the payload fits in one fragment (`K == 1`), [`Self::next_part`]
    /// emits a single-part `ur:<type>/<bytewords>` string (BCR-2024-001).
    ///
    /// # Errors
    ///
    /// Propagates type validation and fountain construction errors.
    pub fn new(message: &[u8], max_fragment_length: usize, ur_type: &UrType) -> Result<Self> {
        Ok(Self {
            fountain: fountain::Encoder::new(message, max_fragment_length)?,
            ur_type: ur_type.clone(),
            message: message.to_vec(),
            single_emitted: 0,
        })
    }

    /// Whether this encoder emits a single-part UR (`K == 1`).
    #[must_use]
    pub const fn is_single_part(&self) -> bool {
        self.fountain.fragment_count() == 1
    }

    /// Emits the next UR string.
    ///
    /// Single-fragment messages (`K == 1`) use the single-part form and re-emit
    /// the same `ur:<type>/<bytewords>` string on every call. The fountain
    /// encoder is not advanced. Larger messages use fountain
    /// `ur:<type>/<seq>-<count>/<bytewords>`.
    ///
    /// # Errors
    ///
    /// Multi-part only: sequence resource limits from the fountain encoder.
    pub fn next_part(&mut self) -> Result<String> {
        if self.is_single_part() {
            self.single_emitted = 1;
            return Ok(encode(&self.message, &self.ur_type));
        }
        let part = self.fountain.next_part()?;
        let body = bytewords::encode(&part.to_cbor(), Style::Minimal);
        Ok(alloc::format!(
            "ur:{}/{}/{body}",
            self.ur_type.as_str(),
            part.sequence_id()
        ))
    }

    /// Current emitted part count.
    ///
    /// Single-part: `0` before the first emit, then `1`. Multi-part: fountain
    /// `seqNum`.
    #[must_use]
    pub const fn current_index(&self) -> u32 {
        if self.is_single_part() {
            self.single_emitted
        } else {
            self.fountain.current_sequence()
        }
    }

    /// Whether a single-part UR has been emitted, or every source fragment has
    /// been emitted at least once.
    #[must_use]
    pub const fn complete(&self) -> bool {
        if self.is_single_part() {
            self.single_emitted == 1
        } else {
            self.fountain.complete()
        }
    }

    /// Source fragment count `K`.
    #[must_use]
    pub const fn fragment_count(&self) -> u32 {
        self.fountain.fragment_count()
    }
}

/// UR decoder (single-part or fountain).
#[derive(Debug)]
pub struct Decoder {
    fountain: fountain::Decoder,
    max_uri_len: usize,
    max_message_length: usize,
    expected_type: Option<UrType>,
    seen_type: Option<UrType>,
    single: Option<Vec<u8>>,
    /// Fail-closed session flag (resource limit or decoder-state).
    poisoned: Option<Poison>,
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
    pub const fn with_limits(limits: DecoderLimits) -> Self {
        Self {
            fountain: fountain::Decoder::with_limits(limits),
            max_uri_len: limits.max_uri_len,
            max_message_length: limits.max_message_length,
            expected_type: None,
            seen_type: None,
            single: None,
            poisoned: None,
        }
    }

    /// Requires every received part to match `ur_type`.
    #[must_use]
    pub fn with_expected_type(mut self, ur_type: UrType) -> Self {
        self.expected_type = Some(ur_type);
        self
    }

    const fn poison(&mut self, kind: ResourceKind) -> Error {
        let poison = Poison::Limit(kind);
        self.poisoned = Some(poison);
        poison.to_error()
    }

    fn escalate(&mut self, err: Error) -> Error {
        match err {
            Error::ResourceLimit(kind) => self.poison(kind),
            Error::DecoderState => {
                self.poisoned = Some(Poison::DecoderState);
                Poison::DecoderState.to_error()
            }
            other => other,
        }
    }

    fn check_type(&self, ur_type: &UrType) -> Result<()> {
        if let Some(ref expected) = self.expected_type {
            if ur_type != expected {
                return Err(Error::UnexpectedType {
                    expected: String::from(expected.as_str()),
                    found: String::from(ur_type.as_str()),
                });
            }
        }
        if let Some(ref seen) = self.seen_type {
            if ur_type != seen {
                return Err(Error::UnexpectedType {
                    expected: String::from(seen.as_str()),
                    found: String::from(ur_type.as_str()),
                });
            }
        }
        Ok(())
    }

    /// Receives one UR string (single-part or fountain part).
    ///
    /// A single-part URI completes the session immediately. Fountain parts are
    /// combined until the message is recovered. The first **successfully**
    /// ingested part pins the type.
    ///
    /// # Errors
    ///
    /// Returns parse, type, index, bytewords, CBOR, or fountain errors.
    /// [`Error::ResourceLimit`] and unrecoverable [`Error::DecoderState`]
    /// poison the session (fail-closed).
    pub fn receive(&mut self, value: &str) -> Result<()> {
        if let Some(poison) = self.poisoned {
            return Err(poison.to_error());
        }

        if value.len() > self.max_uri_len {
            return Err(self.poison(ResourceKind::UriLen));
        }

        let parsed = parse(value)?;
        self.check_type(&parsed.ur_type)?;
        match parsed.kind {
            Kind::SinglePart => self.receive_single(parsed),
            Kind::MultiPart => self.receive_fountain(parsed),
        }
    }

    fn receive_single(&mut self, parsed: ParsedUr) -> Result<()> {
        if self.fountain.resolved_fragment_count().is_some() {
            return Err(Error::InconsistentPart);
        }
        if self.single.is_some() {
            return Ok(());
        }
        let data = bytewords::decode(&parsed.body, Style::Minimal)?;
        if data.len() > self.max_message_length {
            return Err(self.poison(ResourceKind::MessageLength));
        }
        self.seen_type = Some(parsed.ur_type);
        self.single = Some(data);
        Ok(())
    }

    fn receive_fountain(&mut self, parsed: ParsedUr) -> Result<()> {
        if self.single.is_some() {
            return Err(Error::InconsistentPart);
        }
        let decoded = bytewords::decode(&parsed.body, Style::Minimal)?;
        let part = fountain::Part::from_cbor_with_max(
            decoded.as_slice(),
            self.fountain.max_fragment_data_length(),
            self.fountain.max_fragment_count(),
        )
        .map_err(|e| self.escalate(e))?;
        let (idx, idx_total) = parsed.indices.ok_or(Error::InvalidIndices)?;
        if part.sequence() != idx || part.sequence_count() != idx_total {
            return Err(Error::InvalidIndices);
        }
        self.fountain.receive(part).map_err(|e| self.escalate(e))?;
        if self.seen_type.is_none() {
            self.seen_type = Some(parsed.ur_type);
        }
        Ok(())
    }

    /// Whether the message is fully recovered.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.single.is_some() || self.fountain.complete()
    }

    /// Type pinned by the first successfully received part.
    #[must_use]
    pub const fn ur_type(&self) -> Option<&UrType> {
        self.seen_type.as_ref()
    }

    /// Returns the decoded message if complete.
    ///
    /// # Errors
    ///
    /// Propagates fountain message errors. Resource-limit sessions stay fail-closed.
    pub fn message(&self) -> Result<Option<Vec<u8>>> {
        if let Some(poison) = self.poisoned {
            return Err(poison.to_error());
        }
        if let Some(ref data) = self.single {
            return Ok(Some(data.clone()));
        }
        self.fountain.message()
    }

    /// Resolved source fragment count, or `None` before any part.
    #[must_use]
    pub fn resolved_fragment_count(&self) -> Option<u32> {
        if self.single.is_some() {
            Some(1)
        } else {
            self.fountain.resolved_fragment_count()
        }
    }

    /// Total fragment count `K` (0 before any part).
    #[must_use]
    pub const fn fragment_count(&self) -> u32 {
        if self.single.is_some() {
            1
        } else {
            self.fountain.fragment_count()
        }
    }

    /// Whether this session is fail-closed (`ResourceLimit` or `DecoderState`,
    /// including the inner fountain decoder).
    #[must_use]
    pub const fn is_poisoned(&self) -> bool {
        self.poisoned.is_some() || self.fountain.is_poisoned()
    }
}

/// Uppercase UR string for QR efficiency.
#[must_use]
pub fn qr_string(ur: &str) -> String {
    ur.to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use minicbor::bytes::ByteVec;

    use super::*;
    use crate::fountain::DecoderLimits;
    use crate::rng::test_utils::make_message;

    fn make_message_ur(length: usize, seed: &str) -> Vec<u8> {
        let message = make_message(seed, length);
        minicbor::to_vec(ByteVec::from(message)).unwrap()
    }

    fn testdata_lines(raw: &str) -> Vec<&str> {
        raw.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect()
    }

    #[test]
    fn test_single_part_ur() {
        let ur = make_message_ur(50, "Wolf");
        let encoded = encode(&ur, &UrType::bytes());
        let expected = "ur:bytes/hdeymejtswhhylkepmykhhtsytsnoyoyaxaedsuttydmmhhpktpmsrjtgwdpfnsboxgwlbaawzuefywkdplrsrjynbvygabwjldapfcsdwkbrkch";
        assert_eq!(encoded, expected);
        let decoded = decode(&encoded).unwrap();
        assert_eq!((Kind::SinglePart, ur), decoded);
    }

    #[test]
    fn test_ur_encoder() {
        // Full 20-URI table from ur-rs 0.5 `test_ur_encoder` (MIT).
        let ur = make_message_ur(256, "Wolf");
        let mut encoder = Encoder::bytes(&ur, 30).unwrap();
        let expected = testdata_lines(include_str!("../../tests/vectors/ur_rs_multipart_20.txt"));
        assert_eq!(expected.len(), 20);
        assert_eq!(encoder.fragment_count(), 9);
        for (index, e) in expected.into_iter().enumerate() {
            assert_eq!(encoder.current_index() as usize, index);
            assert_eq!(encoder.next_part().unwrap(), e);
        }
    }

    #[test]
    fn test_ur_encoder_decoder_bc_crypto_request() {
        // ur-rs / Blockchain Commons crypto-request seed vector.
        fn crypto_seed() -> Vec<u8> {
            let mut e = minicbor::Encoder::new(Vec::new());
            let uuid = hex::decode("020C223A86F7464693FC650EF3CAC047").unwrap();
            let seed_digest =
                hex::decode("E824467CAFFEAF3BBC3E0CA095E660A9BAD80DDB6A919433A37161908B9A3986")
                    .unwrap();
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
            e.into_writer()
        }

        let data = crypto_seed();
        let encoded = encode(&data, &UrType::new("crypto-request").unwrap());
        let expected = "ur:crypto-request/oeadtpdagdaobncpftlnylfgfgmuztihbawfsgrtflaotaadwkoyadtaaohdhdcxvsdkfgkepezepefrrffmbnnbmdvahnptrdtpbtuyimmemweootjshsmhlunyeslnameyhsdi";
        assert_eq!(encoded, expected);
        let decoded = decode(&encoded).unwrap();
        assert_eq!((Kind::SinglePart, data), decoded);
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
            encode(b"data", &UrType::bytes()),
            "ur:bytes/iehsjyhspmwfwfia"
        );
    }

    #[test]
    fn test_case_fold() {
        let lower = encode(b"data", &UrType::bytes());
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
        let mut encoder = Encoder::new(data, 5, &UrType::new("my-scheme").unwrap()).unwrap();
        assert_eq!(
            encoder.next_part().unwrap(),
            "ur:my-scheme/1-2/lpadaobkcywkwmhfwnfeghihjtcxiansvomopr"
        );
    }

    #[test]
    fn test_single_part_receive_completes() {
        let mut decoder = Decoder::default();
        decoder.receive("ur:bytes/iehsjyhspmwfwfia").unwrap();
        assert!(decoder.complete());
        assert_eq!(
            decoder.message().unwrap().as_deref(),
            Some(b"data".as_slice())
        );
    }

    #[test]
    fn test_encoder_k1_is_single_part() {
        let data = b"hello";
        let mut encoder = Encoder::bytes(data, 64).unwrap();
        assert!(encoder.is_single_part());
        let part = encoder.next_part().unwrap();
        assert!(!part.contains("/1-1/"));
        assert_eq!(part, encode(data, &UrType::bytes()));
        let mut decoder = Decoder::default();
        decoder.receive(&part).unwrap();
        assert_eq!(decoder.message().unwrap().as_deref(), Some(data.as_slice()));
    }

    #[test]
    fn test_encoder_k1_next_part_is_idempotent() {
        let data = b"hello";
        let mut encoder = Encoder::bytes(data, 64).unwrap();
        assert!(!encoder.complete());
        assert_eq!(encoder.current_index(), 0);
        let first = encoder.next_part().unwrap();
        assert_eq!(first, encode(data, &UrType::bytes()));
        assert_eq!(encoder.current_index(), 1);
        assert!(encoder.complete());
        let second = encoder.next_part().unwrap();
        assert_eq!(second, first);
        assert_eq!(encoder.current_index(), 1);
        for _ in 0..10 {
            assert_eq!(encoder.next_part().unwrap(), first);
            assert_eq!(encoder.current_index(), 1);
            assert!(encoder.complete());
        }
    }

    #[test]
    fn test_foreign_1_1_fountain_uri_decodes() {
        let mut fountain = fountain::Encoder::new(b"hello", 64).unwrap();
        let part = fountain.next_part().unwrap();
        let body = bytewords::encode(&part.to_cbor(), Style::Minimal);
        let uri = alloc::format!("ur:bytes/1-1/{body}");
        let mut decoder = Decoder::default();
        decoder.receive(&uri).unwrap();
        assert!(decoder.complete());
        assert_eq!(
            decoder.message().unwrap().as_deref(),
            Some(b"hello".as_slice())
        );
    }

    #[test]
    fn test_decode_message_rejects_multipart() {
        let data = b"Ten chars!".repeat(8);
        let mut encoder = Encoder::bytes(&data, 5).unwrap();
        let part = encoder.next_part().unwrap();
        assert!(matches!(decode_message(&part), Err(Error::NotSinglePart)));
        assert_eq!(
            decode_message(&encode(b"data", &UrType::bytes())).unwrap(),
            b"data"
        );
    }

    #[test]
    fn test_garbage_does_not_pin_type() {
        let data = b"Ten chars!".repeat(6);
        let mut encoder = Encoder::new(&data, 5, &UrType::new("alpha").unwrap()).unwrap();
        let mut decoder = Decoder::default();
        assert!(decoder.receive("ur:beta/1-2/zzzz").is_err());
        assert!(decoder.ur_type().is_none());
        decoder.receive(&encoder.next_part().unwrap()).unwrap();
        assert_eq!(decoder.ur_type().map(UrType::as_str), Some("alpha"));
    }

    #[test]
    fn test_bc_ur_example() {
        // CBOR array [1, 2, 3] as single-part ur:test (bc-ur golden)
        // We only check roundtrip of raw CBOR bytes through bytewords path
        // using the known string from bc-ur docs when payload is correct CBOR.
        let cbor = hex::decode("83010203").unwrap(); // array(3) [1,2,3]
        let ur = encode(&cbor, &UrType::new("test").unwrap());
        assert_eq!(ur, "ur:test/lsadaoaxjygonesw");
        let (kind, data) = decode(&ur).unwrap();
        assert_eq!(kind, Kind::SinglePart);
        assert_eq!(data, cbor);
    }

    #[test]
    fn test_parse_and_decode_with_type() {
        let ur = encode(b"data", &UrType::bytes());
        let parsed = parse(&ur).unwrap();
        assert_eq!(parsed.kind, Kind::SinglePart);
        assert_eq!(parsed.ur_type.as_str(), "bytes");
        assert!(parsed.indices.is_none());

        let (ty, kind, payload) = decode_with_type(&ur).unwrap();
        assert_eq!(ty.as_str(), "bytes");
        assert_eq!(kind, Kind::SinglePart);
        assert_eq!(payload, b"data");
    }

    #[test]
    fn test_into_ur_type_accepts_owned_and_borrowed() {
        let t = UrType::new("bytes").unwrap();
        assert_eq!(t.clone().into_ur_type().unwrap(), t);
        assert_eq!(IntoUrType::into_ur_type(&t).unwrap(), t);
        assert_eq!("bytes".into_ur_type().unwrap(), t);
        assert_eq!(String::from("bytes").into_ur_type().unwrap(), t);
        assert!(matches!("".into_ur_type(), Err(Error::InvalidType)));
    }

    #[test]
    fn test_invalid_type_and_indices() {
        assert!(matches!(UrType::new(""), Err(Error::InvalidType)));
        assert!(matches!(UrType::new("Bad_Type"), Err(Error::InvalidType)));
        assert!(matches!(
            parse("ur:bytes/0-1/aeadaolazmjendeoti"),
            Err(Error::InvalidIndices)
        ));
        assert!(matches!(
            parse("ur:bytes/1-0/aeadaolazmjendeoti"),
            Err(Error::InvalidIndices)
        ));
        assert!(matches!(
            parse("ur:bytes/foo/aeadaolazmjendeoti"),
            Err(Error::InvalidIndices)
        ));
    }

    #[test]
    fn test_expected_type_and_uri_limit() {
        let data = b"Ten chars!".repeat(5);
        let mut enc = Encoder::new(&data, 5, &UrType::new("alpha").unwrap()).unwrap();
        let part = enc.next_part().unwrap();

        let mut decoder = Decoder::default().with_expected_type(UrType::new("beta").unwrap());
        assert!(matches!(
            decoder.receive(&part),
            Err(Error::UnexpectedType { .. })
        ));

        let limits = DecoderLimits {
            max_uri_len: 8,
            ..DecoderLimits::default()
        };
        let mut short = Decoder::with_limits(limits);
        assert!(matches!(
            short.receive(&part),
            Err(Error::ResourceLimit(ResourceKind::UriLen))
        ));
    }

    #[test]
    fn test_multipart_index_mismatch() {
        let data = b"Ten chars!".repeat(5);
        let mut enc = Encoder::bytes(&data, 5).unwrap();
        let part = enc.next_part().unwrap();
        // Corrupt path indices while keeping a valid multi-part shape.
        let corrupted = part.replacen("/1-", "/2-", 1);
        let mut decoder = Decoder::default();
        assert!(matches!(
            decoder.receive(&corrupted),
            Err(Error::InvalidIndices)
        ));
    }

    #[test]
    fn test_empty_single_part() {
        let ur = encode(&[], &UrType::bytes());
        let (kind, payload) = decode(&ur).unwrap();
        assert_eq!(kind, Kind::SinglePart);
        assert!(payload.is_empty());
    }

    #[test]
    fn test_parse_folds_body() {
        let parsed = parse("ur:bytes/IEHSJYHSPMWFWFIA").unwrap();
        assert_eq!(parsed.body, "iehsjyhspmwfwfia");
        assert_eq!(parsed.ur_type.as_str(), "bytes");
    }

    #[test]
    fn test_uri_len_resource_limit_poisons() {
        let data = b"Ten chars!".repeat(5);
        let mut enc = Encoder::bytes(&data, 5).unwrap();
        let part = enc.next_part().unwrap();

        let limits = DecoderLimits {
            max_uri_len: 8,
            ..DecoderLimits::default()
        };
        let mut decoder = Decoder::with_limits(limits);
        assert!(matches!(
            decoder.receive(&part),
            Err(Error::ResourceLimit(ResourceKind::UriLen))
        ));
        assert!(decoder.is_poisoned());
        assert!(matches!(
            decoder.receive(&part),
            Err(Error::ResourceLimit(ResourceKind::UriLen))
        ));
        assert!(matches!(
            decoder.message(),
            Err(Error::ResourceLimit(ResourceKind::UriLen))
        ));
    }

    #[test]
    fn test_decoder_progress_accessors() {
        let ur = make_message_ur(256, "Wolf");
        let mut encoder = Encoder::bytes(&ur, 30).unwrap();
        let mut decoder = Decoder::default();
        assert_eq!(decoder.resolved_fragment_count(), None);
        assert_eq!(decoder.fragment_count(), 0);

        decoder.receive(&encoder.next_part().unwrap()).unwrap();
        assert_eq!(decoder.resolved_fragment_count(), Some(1));
        assert_eq!(decoder.fragment_count(), encoder.fragment_count());

        let mut prev = 1;
        while !decoder.complete() {
            decoder.receive(&encoder.next_part().unwrap()).unwrap();
            let now = decoder.resolved_fragment_count().unwrap();
            assert!(now >= prev);
            assert!(now <= decoder.fragment_count());
            prev = now;
        }
        assert_eq!(
            decoder.resolved_fragment_count(),
            Some(decoder.fragment_count())
        );
    }
}
