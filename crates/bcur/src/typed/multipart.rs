//! Owned multi-part wrappers over [`crate::Encoder`] / [`crate::Decoder`].

use dcbor::CBOR;

use super::{Ur, map_cbor};
use crate::{CborErrorKind, DecoderLimits, Error, Result};

/// Fountain encoder that owns the CBOR payload bytes (no lifetime on [`Ur`]).
#[derive(Debug)]
pub struct MultipartEncoder {
    encoder: crate::Encoder,
}

impl MultipartEncoder {
    /// Starts an encode session for `ur` with a maximum fragment length.
    ///
    /// When the payload fits in one fragment (`K == 1`), [`Self::next_part`]
    /// emits a single-part `ur:<type>/<bytewords>` string.
    ///
    /// # Errors
    ///
    /// Propagates fountain construction errors (`EmptyMessage`,
    /// `InvalidFragmentLen`, size-to-`u32` limits).
    pub fn new(ur: &Ur, max_fragment_len: usize) -> Result<Self> {
        Ok(Self {
            encoder: crate::Encoder::new(
                &ur.cbor().to_cbor_data(),
                max_fragment_len,
                ur.ur_type(),
            )?,
        })
    }

    /// Next UR string: single-part when `K == 1`, otherwise
    /// `ur:<type>/<seq>-<count>/<bytewords>`.
    ///
    /// # Errors
    ///
    /// [`Error::ResourceLimit`] ([`crate::ResourceKind::Sequence`]) after
    /// `u32::MAX` parts.
    pub fn next_part(&mut self) -> Result<String> {
        self.encoder.next_part()
    }

    /// Number of parts emitted so far.
    #[must_use]
    pub const fn current_index(&self) -> u32 {
        self.encoder.current_index()
    }

    /// Source fragment count `K`.
    #[must_use]
    pub const fn fragment_count(&self) -> u32 {
        self.encoder.fragment_count()
    }
}

/// Fountain decoder that rebuilds a [`Ur`] from recovered dCBOR bytes.
#[derive(Debug)]
pub struct MultipartDecoder {
    decoder: crate::Decoder,
}

impl MultipartDecoder {
    /// Decoder with [`DecoderLimits::default`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(DecoderLimits::default())
    }

    /// Decoder with explicit resource limits.
    #[must_use]
    pub const fn with_limits(limits: DecoderLimits) -> Self {
        Self {
            decoder: crate::Decoder::with_limits(limits),
        }
    }

    /// Receives one UR string (single-part or fountain; type stickiness).
    ///
    /// # Errors
    ///
    /// Same as [`crate::Decoder::receive`]: parse, type, index, bytewords,
    /// part CBOR, fountain, or resource-limit (fail-closed).
    pub fn receive(&mut self, value: &str) -> Result<()> {
        self.decoder.receive(value)
    }

    /// Whether the original payload is fully recovered.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.decoder.complete()
    }

    /// Reconstructed [`Ur`] when complete; `Ok(None)` while still receiving.
    ///
    /// # Errors
    ///
    /// Fountain join errors, [`Error::Cbor`] if recovered bytes are not dCBOR,
    /// or [`Error::DecoderState`] if complete without a pinned type.
    pub fn message(&self) -> Result<Option<Ur>> {
        let Some(data) = self.decoder.message()? else {
            return Ok(None);
        };
        let ur_type = self.decoder.ur_type().ok_or(Error::DecoderState)?;
        let cbor = map_cbor(CBOR::try_from_data(data), CborErrorKind::Decode)?;
        Ur::new(ur_type, cbor).map(Some)
    }
}

impl Default for MultipartDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    fn large_ur() -> Ur {
        Ur::new("alpha", (0_u8..80).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn drop_odd_parts_roundtrip() {
        let ur = large_ur();
        let mut encoder = MultipartEncoder::new(&ur, 10).unwrap();
        let mut decoder = MultipartDecoder::new();
        assert!(encoder.fragment_count() > 1);
        assert!(decoder.message().unwrap().is_none());

        while !decoder.complete() {
            let part = encoder.next_part().unwrap();
            if encoder.current_index() & 1 == 1 {
                decoder.receive(&part).unwrap();
            }
        }

        let recovered = decoder.message().unwrap().unwrap();
        assert_eq!(recovered.ur_type_str(), "alpha");
        assert_eq!(recovered.cbor(), ur.cbor());
    }

    #[test]
    fn type_mismatch_on_second_part() {
        let payload = (0_u8..80).collect::<Vec<_>>();
        let alpha = Ur::new("alpha", payload.clone()).unwrap();
        let beta = Ur::new("beta", payload).unwrap();
        let mut enc_a = MultipartEncoder::new(&alpha, 10).unwrap();
        let mut enc_b = MultipartEncoder::new(&beta, 10).unwrap();
        let mut decoder = MultipartDecoder::default();
        decoder.receive(&enc_a.next_part().unwrap()).unwrap();
        assert!(matches!(
            decoder.receive(&enc_b.next_part().unwrap()).unwrap_err(),
            Error::UnexpectedType { .. }
        ));
    }

    #[test]
    fn uppercase_parts_accepted() {
        let ur = large_ur();
        let mut encoder = MultipartEncoder::new(&ur, 10).unwrap();
        let mut decoder = MultipartDecoder::new();
        while !decoder.complete() {
            let part = encoder.next_part().unwrap();
            decoder.receive(&part.to_ascii_uppercase()).unwrap();
        }
        assert_eq!(decoder.message().unwrap().unwrap().cbor(), ur.cbor());
    }
}
