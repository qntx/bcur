//! Public-API integration checks compiled as library unit tests.
//!
//! Kept under `src/` so they participate in normal `#[cfg(test)]` lint rules
//! (unwrap-in-tests, no phantom package-level dependencies).

mod adversarial_decoder;
mod interop_bc_ur;
mod interop_ur_rs;
