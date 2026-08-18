//! Unified error type for the `bcur` crate.

use alloc::string::String;

/// Errors that can occur while encoding or decoding Uniform Resources.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
#[allow(
    clippy::error_impl_error,
    reason = "crate-root Error is the standard library-facing error type name"
)]
pub enum Error {
    // --- bytewords ---
    /// An unrecognized or malformed byteword token was encountered.
    #[error("invalid bytewords word")]
    InvalidWord,
    /// The CRC-32 checksum appended to a bytewords payload did not match.
    #[error("invalid bytewords checksum")]
    InvalidBytewordsChecksum,
    /// The bytewords string length is inconsistent with the selected style.
    #[error("invalid bytewords length")]
    InvalidBytewordsLength,
    /// The input contained non-ASCII characters.
    #[error("bytewords string is not ASCII")]
    NonAscii,

    // --- fountain ---
    /// The fountain encoder was given an empty message.
    #[error("empty message")]
    EmptyMessage,
    /// A fountain part had empty payload or invalid empty metadata.
    #[error("empty fountain part")]
    EmptyPart,
    /// Maximum fragment length must be greater than zero.
    #[error("invalid maximum fragment length")]
    InvalidFragmentLen,
    /// Fountain part sequence numbers must be non-zero.
    #[error("invalid sequence number")]
    InvalidSequence,
    /// A part is inconsistent with previously received parts.
    #[error("fountain part inconsistent with previous parts")]
    InconsistentPart,
    /// Joined fragments had non-zero padding past the message length.
    #[error("invalid fountain part padding")]
    InvalidPadding,
    /// Joined fountain payload failed its CRC-32 check.
    #[error("invalid fountain message checksum")]
    InvalidMessageChecksum,
    /// The fountain part CBOR was malformed or non-canonical for our schema.
    #[error("invalid fountain part CBOR")]
    InvalidPartCbor,
    /// Internal decoder invariant was broken.
    #[error("fountain decoder internal state error")]
    DecoderState,
    /// A configured resource limit was exceeded.
    #[error("decoder resource limit exceeded: {0}")]
    ResourceLimit(&'static str),

    // --- UR ---
    /// The URI did not start with the `ur:` scheme.
    #[error("invalid UR scheme")]
    InvalidScheme,
    /// No type token was present after the scheme.
    #[error("UR type unspecified")]
    TypeUnspecified,
    /// The UR type token is empty or contains illegal characters.
    #[error("invalid UR type")]
    InvalidType,
    /// Multi-part sequence indices were missing or inconsistent with the part.
    #[error("invalid multi-part indices")]
    InvalidIndices,
    /// A single-part API was used on a multi-part UR.
    #[error("expected single-part UR")]
    NotSinglePart,
    /// The UR type did not match the expected type.
    #[error("unexpected UR type: expected {expected}, found {found}")]
    UnexpectedType {
        /// Expected type string.
        expected: String,
        /// Found type string.
        found: String,
    },

    // --- typed / dCBOR ---
    /// An error from the optional dCBOR layer.
    #[error("dCBOR error: {0}")]
    Cbor(String),
}

/// Result alias for `bcur` operations.
pub type Result<T> = core::result::Result<T, Error>;
