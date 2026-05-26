//! Compile-pass fixtures for the contract macro.
//!
//! Each fixture pins a valid contract shape that `tests/test-contract/`
//! does **not** exercise. See `tests/README.md` for the per-fixture
//! conventions (`// Pins:` header, topic taxonomy).

#![no_std]

pub mod events;
pub mod methods;
