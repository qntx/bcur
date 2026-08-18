//! CLI error type.

use std::fmt;
use std::io;

/// Failures from argument handling, I/O, transport, or QR rendering.
#[derive(Debug)]
#[allow(
    clippy::error_impl_error,
    reason = "CLI Error is the binary-facing error type name"
)]
pub(crate) enum Error {
    /// Wrapped `bcur` transport error.
    Bcur(bcur::Error),
    /// Filesystem or terminal I/O.
    Io(io::Error),
    /// Hex decode of `--hex` input.
    Hex(hex::FromHexError),
    /// QR construction or density.
    Qr(String),
    /// Usage or incomplete decode.
    Msg(String),
}

impl Error {
    pub(crate) fn msg(msg: impl Into<String>) -> Self {
        Self::Msg(msg.into())
    }

    pub(crate) fn qr(msg: impl Into<String>) -> Self {
        Self::Qr(msg.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bcur(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Hex(e) => write!(f, "hex: {e}"),
            Self::Qr(e) | Self::Msg(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bcur(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Hex(e) => Some(e),
            Self::Qr(_) | Self::Msg(_) => None,
        }
    }
}

impl From<bcur::Error> for Error {
    fn from(value: bcur::Error) -> Self {
        Self::Bcur(value)
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<hex::FromHexError> for Error {
    fn from(value: hex::FromHexError) -> Self {
        Self::Hex(value)
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
