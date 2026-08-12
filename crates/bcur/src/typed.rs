//! Optional typed dCBOR layer (feature = `dcbor`).
//!
//! Full `Ur` value objects and codable traits land in 0.2.0. This module keeps
//! the feature graph valid and documents the intended extension point.

// Ensure the optional dependency is linked while the typed API is still a stub.
use dcbor as _;
