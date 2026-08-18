//! Unified error type for the `bcur` crate.

use alloc::string::String;

/// Which decoder/encoder budget was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResourceKind {
    /// Original payload length (`max_message_length` or `u32` wire field).
    MessageLength,
    /// Fragment count `K` (`max_fragment_count` or `u32` wire field).
    FragmentCount,
    /// Fountain `seqNum` would exceed `u32::MAX`.
    Sequence,
    /// Part payload length (`max_fragment_data_length`).
    FragmentData,
    /// Unique received index-set count (`max_received_parts`).
    ReceivedParts,
    /// Mixed-part XOR buffer size (`max_buffer_parts`).
    BufferParts,
    /// UR string length (`max_uri_len`).
    UriLen,
}

/// dCBOR failure from the typed layer. Transport never produces this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CborError {
    kind: CborErrorKind,
    detail: String,
}

impl CborError {
    #[cfg_attr(
        not(feature = "dcbor"),
        allow(dead_code, reason = "typed layer is the only constructor caller")
    )]
    pub(crate) fn new(kind: CborErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    /// Failure class.
    #[must_use]
    pub const fn kind(&self) -> CborErrorKind {
        self.kind
    }

    /// Underlying `dcbor` display text.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// Class of dCBOR failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CborErrorKind {
    /// `CBOR::try_from_data` / well-formedness / determinism.
    Decode,
    /// Well-formed CBOR that cannot become the requested Rust type.
    Type,
}

/// Fail-closed decoder poison. Not re-exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Poison {
    Limit(ResourceKind),
    DecoderState,
}

impl Poison {
    pub(crate) const fn to_error(self) -> Error {
        match self {
            Self::Limit(kind) => Error::ResourceLimit(kind),
            Self::DecoderState => Error::DecoderState,
        }
    }
}

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
    /// Fountain `next_part` called again when `K == 1`.
    #[error("single-part fountain encoder already emitted its only part")]
    SinglePartExhausted,
    /// A configured resource limit was exceeded.
    #[error("decoder resource limit exceeded: {0:?}")]
    ResourceLimit(ResourceKind),

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
    #[error("dCBOR error ({kind:?}): {detail}", kind = .0.kind(), detail = .0.detail())]
    Cbor(CborError),
}

/// Result alias for `bcur` operations.
pub type Result<T> = core::result::Result<T, Error>;
