//! Encode/decode traits for tagged dCBOR types.

use dcbor::{CBORTagged, CBORTaggedDecodable, CBORTaggedEncodable};

use super::{Ur, map_cbor};
use crate::{CborErrorKind, Error, Result, UrType};

/// First registered CBOR tag name, validated as a UR type.
fn first_tag_ur_type<T: CBORTagged>() -> Result<UrType> {
    let name = T::cbor_tags()
        .first()
        .and_then(dcbor::Tag::name)
        .ok_or(Error::InvalidType)?;
    UrType::new(&name)
}

/// Encode as a UR using the first registered CBOR tag **name** as the type.
///
/// The payload is **untagged** dCBOR. Missing or unnamed tags yield
/// [`Error::InvalidType`] — this method never panics.
pub trait UrEncodable {
    /// Typed UR for this value.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidType`] if the first `cbor_tags()` entry has no name or
    /// the name is not a valid UR type token.
    fn ur(&self) -> Result<Ur>;

    /// Single-part UR string.
    ///
    /// # Errors
    ///
    /// Same as [`Self::ur`].
    fn ur_string(&self) -> Result<String> {
        Ok(self.ur()?.string())
    }
}

/// Decode from a typed UR using untagged dCBOR.
pub trait UrDecodable: Sized {
    /// Decode from an already-parsed UR.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidType`] if this type's first tag is unnamed;
    /// [`Error::UnexpectedType`] if the UR type does not match that name;
    /// [`Error::Cbor`] if untagged decode fails.
    fn from_ur(ur: &Ur) -> Result<Self>;

    /// Parse a single-part UR string and decode it.
    ///
    /// # Errors
    ///
    /// [`Ur::from_ur_string`] errors, then [`Self::from_ur`].
    fn from_ur_string(s: impl AsRef<str>) -> Result<Self> {
        Self::from_ur(&Ur::from_ur_string(s)?)
    }
}

/// Types that both encode to and decode from typed URs.
pub trait UrCodable: UrEncodable + UrDecodable {}

impl<T: CBORTaggedEncodable> UrEncodable for T {
    fn ur(&self) -> Result<Ur> {
        Ur::new(&first_tag_ur_type::<T>()?, self.untagged_cbor())
    }
}

impl<T: CBORTaggedDecodable> UrDecodable for T {
    fn from_ur(ur: &Ur) -> Result<Self> {
        ur.check_type(&first_tag_ur_type::<T>()?)?;
        map_cbor(
            Self::from_untagged_cbor(ur.cbor().clone()),
            CborErrorKind::Type,
        )
    }
}

impl<T: UrEncodable + UrDecodable> UrCodable for T {}

#[cfg(test)]
mod tests {
    use dcbor::{CBOR, CBORTagged, CBORTaggedDecodable, CBORTaggedEncodable, Tag};

    use super::*;
    use crate::Error;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct NamedNote(String);

    impl CBORTagged for NamedNote {
        fn cbor_tags() -> Vec<Tag> {
            vec![Tag::with_static_name(40_000, "note")]
        }
    }

    impl CBORTaggedEncodable for NamedNote {
        fn untagged_cbor(&self) -> CBOR {
            self.0.clone().into()
        }
    }

    impl CBORTaggedDecodable for NamedNote {
        fn from_untagged_cbor(cbor: CBOR) -> dcbor::Result<Self> {
            Ok(Self(cbor.try_into()?))
        }
    }

    impl TryFrom<CBOR> for NamedNote {
        type Error = dcbor::Error;

        fn try_from(cbor: CBOR) -> dcbor::Result<Self> {
            Self::from_tagged_cbor(cbor)
        }
    }

    #[derive(Debug)]
    struct UnnamedByte(u8);

    impl CBORTagged for UnnamedByte {
        fn cbor_tags() -> Vec<Tag> {
            vec![Tag::with_value(40_001)]
        }
    }

    impl CBORTaggedEncodable for UnnamedByte {
        fn untagged_cbor(&self) -> CBOR {
            self.0.into()
        }
    }

    #[derive(Debug)]
    struct EmptyTags;

    impl CBORTagged for EmptyTags {
        fn cbor_tags() -> Vec<Tag> {
            Vec::new()
        }
    }

    impl CBORTaggedEncodable for EmptyTags {
        fn untagged_cbor(&self) -> CBOR {
            0_u8.into()
        }
    }

    #[test]
    fn named_tag_roundtrip() {
        let note = NamedNote(String::from("hi"));
        let ur = note.ur().unwrap();
        assert_eq!(ur.ur_type_str(), "note");
        assert_eq!(note.ur_string().unwrap(), ur.string());
        let decoded = NamedNote::from_ur(&ur).unwrap();
        assert_eq!(decoded, note);
        let via_string = NamedNote::from_ur_string(ur.string()).unwrap();
        assert_eq!(via_string, note);
    }

    #[test]
    fn unnamed_or_empty_tags_are_invalid_type() {
        assert_eq!(UnnamedByte(1).ur().unwrap_err(), Error::InvalidType);
        assert_eq!(EmptyTags.ur().unwrap_err(), Error::InvalidType);
    }

    #[test]
    fn from_ur_rejects_wrong_type() {
        let ur = Ur::new("bytes", NamedNote(String::from("x")).untagged_cbor()).unwrap();
        assert!(matches!(
            NamedNote::from_ur(&ur).unwrap_err(),
            Error::UnexpectedType { .. }
        ));
    }
}
