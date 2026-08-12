//! `bcur` — a Rust implementation of [Uniform Resources](https://github.com/BlockchainCommons/Research/blob/master/papers/bcr-2020-005-ur.md).
//!
//! # Features
//!
//! - **`std`** (default): host builds.
//! - **`dcbor`**: optional typed dCBOR layer (implies `std`; available from 0.2).
//! - **`bytemoji`**: optional bytemoji helpers.
//!
//! Core transport (bytewords, fountain, UR) always requires `alloc` and supports
//! `no_std` via `--no-default-features`.
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
    Decoder, Encoder, Kind, ParsedUr, UrType, decode, decode_with_type, encode, normalize_ur,
    parse, parse_normalized, qr_string,
};

#[cfg(feature = "dcbor")]
pub mod typed;

#[cfg(test)]
mod integration;
