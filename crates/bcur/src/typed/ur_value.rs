//! [`Ur`] value object: validated type plus untagged dCBOR payload.

use std::fmt;

use dcbor::CBOR;

use super::map_cbor;
use crate::ur::Kind;
use crate::{Error, Result, UrType};

/// A Uniform Resource whose payload is deterministic CBOR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ur {
    ur_type: UrType,
    cbor: CBOR,
}

impl Ur {
    /// Builds a UR from a type token and a dCBOR value.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidType`] if `ur_type` is empty or not
    /// `[a-z0-9-]+` after ASCII lowercasing.
    pub fn new(
        ur_type: impl TryInto<UrType, Error = Error>,
        cbor: impl Into<CBOR>,
    ) -> Result<Self> {
        Ok(Self {
            ur_type: ur_type.try_into()?,
            cbor: cbor.into(),
        })
    }

    /// Parses a **single-part** UR string into typed dCBOR.
    ///
    /// The full URI is ASCII-lowercased (QR uppercase accepted).
    ///
    /// # Errors
    ///
    /// Transport parse/decode errors, [`Error::NotSinglePart`] for multi-part
    /// URIs, or [`Error::Cbor`] if the payload is not dCBOR.
    pub fn from_ur_string(s: impl AsRef<str>) -> Result<Self> {
        let (ur_type, kind, data) = crate::decode_with_type(s.as_ref())?;
        if kind != Kind::SinglePart {
            return Err(Error::NotSinglePart);
        }
        Ok(Self {
            ur_type,
            cbor: map_cbor(CBOR::try_from_data(data))?,
        })
    }

    /// Single-part `ur:<type>/<bytewords>` string (lowercase).
    ///
    /// Infallible: the stored type is already validated.
    #[must_use]
    pub fn string(&self) -> String {
        crate::encode(&self.cbor.to_cbor_data(), &self.ur_type)
    }

    /// Uppercase form of [`Self::string`] for QR payloads.
    #[must_use]
    pub fn qr_string(&self) -> String {
        self.string().to_ascii_uppercase()
    }

    /// UTF-8 bytes of [`Self::qr_string`].
    #[must_use]
    pub fn qr_data(&self) -> Vec<u8> {
        self.qr_string().into_bytes()
    }

    /// Requires this UR's type to equal `expected`.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidType`] if `expected` is not a valid type token;
    /// [`Error::UnexpectedType`] on mismatch.
    pub fn check_type(&self, expected: impl TryInto<UrType, Error = Error>) -> Result<()> {
        let expected = expected.try_into()?;
        if self.ur_type == expected {
            Ok(())
        } else {
            Err(Error::UnexpectedType {
                expected: String::from(expected.as_str()),
                found: String::from(self.ur_type.as_str()),
            })
        }
    }

    /// Validated UR type.
    #[must_use]
    pub const fn ur_type(&self) -> &UrType {
        &self.ur_type
    }

    /// UR type as a lowercase `&str`.
    #[must_use]
    pub fn ur_type_str(&self) -> &str {
        self.ur_type.as_str()
    }

    /// Borrow the untagged dCBOR payload.
    #[must_use]
    pub const fn cbor(&self) -> &CBOR {
        &self.cbor
    }

    /// Consume the UR and return the dCBOR payload.
    #[must_use]
    pub fn into_cbor(self) -> CBOR {
        self.cbor
    }
}

impl fmt::Display for Ur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.string())
    }
}

impl From<Ur> for String {
    fn from(ur: Ur) -> Self {
        ur.string()
    }
}

impl From<Ur> for CBOR {
    fn from(ur: Ur) -> Self {
        ur.into_cbor()
    }
}

impl TryFrom<String> for Ur {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::from_ur_string(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytewords::{self, Style};

    const GOLDEN: &str = "ur:test/lsadaoaxjygonesw";

    #[test]
    fn array_123_matches_published_golden() {
        let ur = Ur::new("test", vec![1, 2, 3]).unwrap();
        assert_eq!(ur.string(), GOLDEN);
        assert_eq!(ur.ur_type_str(), "test");
        assert_eq!(ur.to_string(), GOLDEN);

        let parsed = Ur::from_ur_string(GOLDEN).unwrap();
        assert_eq!(parsed, ur);
        assert_eq!(parsed.cbor(), ur.cbor());

        let upper = Ur::from_ur_string("UR:TEST/LSADAOAXJYGONESW").unwrap();
        assert_eq!(upper, ur);
        assert_eq!(ur.qr_string(), "UR:TEST/LSADAOAXJYGONESW");
        assert_eq!(ur.qr_data(), b"UR:TEST/LSADAOAXJYGONESW");
    }

    #[test]
    fn rejects_empty_and_illegal_types() {
        assert_eq!(Ur::new("", vec![1]).unwrap_err(), Error::InvalidType);
        assert_eq!(
            Ur::new("Bad_Type", vec![1]).unwrap_err(),
            Error::InvalidType
        );
    }

    #[test]
    fn from_ur_string_rejects_multipart_and_bad_cbor() {
        let ur = Ur::new("test", (0_u8..64).collect::<Vec<_>>()).unwrap();
        let mut encoder = crate::Encoder::new(&ur.cbor().to_cbor_data(), 12, ur.ur_type()).unwrap();
        let part = encoder.next_part().unwrap();
        assert_eq!(Ur::from_ur_string(&part).unwrap_err(), Error::NotSinglePart);

        let body = bytewords::encode(&[0xff, 0xff], Style::Minimal);
        let uri = format!("ur:test/{body}");
        assert!(matches!(
            Ur::from_ur_string(uri).unwrap_err(),
            Error::Cbor(_)
        ));
    }

    #[test]
    fn check_type_and_into_cbor() {
        let ur = Ur::new("test", vec![1, 2, 3]).unwrap();
        ur.check_type("test").unwrap();
        ur.check_type(ur.ur_type()).unwrap();
        assert!(matches!(
            ur.check_type("bytes").unwrap_err(),
            Error::UnexpectedType { .. }
        ));
        let cbor: CBOR = ur.clone().into();
        assert_eq!(cbor, *ur.cbor());
        let via_string = Ur::try_from(ur.string()).unwrap();
        assert_eq!(via_string, ur);
    }
}
