//! `bcur` — a Rust implementation of [Uniform Resources](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-005-ur.md).
//!
//! # Status
//!
//! **0.2** ships the transport stack (bytewords, fountain codes, multi-part UR,
//! [`DecoderLimits`]) and an optional typed dCBOR layer.
//!
//! # Features
//!
//! - **`std`** (default): host builds.
//! - **`dcbor`**: typed [`typed::Ur`] and [`UrEncodable`] / [`UrDecodable`]
//!   (implies `std`).
//!
//! Core transport always requires `alloc` and supports `no_std` via
//! `--no-default-features`.
//!
//! # Example
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
//!
//! Typed dCBOR (`feature = "dcbor"`):
//!
//! ```
//! # #[cfg(feature = "dcbor")]
//! # {
//! use bcur::Ur;
//! let ur = Ur::new("test", vec![1, 2, 3]).unwrap();
//! assert_eq!(ur.string(), "ur:test/lsadaoaxjygonesw");
//! # }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod bytewords;
pub mod fountain;
pub mod ur;

mod constants;
mod crc32;
mod error;
mod rng;

pub use error::{Error, Result};
pub use fountain::DecoderLimits;
pub use ur::{
    Decoder, Encoder, Kind, ParsedUr, UrType, decode, decode_message, decode_with_type, encode,
    normalize_ur, parse, qr_string,
};

#[cfg(feature = "dcbor")]
pub mod typed;
// Dev-only tools are linked into test/bench targets; keep the lib lint clean.
#[cfg(test)]
use criterion as _;
#[cfg(feature = "dcbor")]
pub use typed::{MultipartDecoder, MultipartEncoder, Ur, UrCodable, UrDecodable, UrEncodable};
