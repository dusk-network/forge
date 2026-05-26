// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Contract building tools for Dusk smart contracts.

#![no_std]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(unused_must_use)]
#![deny(unused_extern_crates)]
#![deny(clippy::pedantic)]
#![warn(missing_debug_implementations, unreachable_pub, rustdoc::all)]

/// Contract schema types and utilities.
pub mod schema;

/// Re-export the contract proc macro.
pub use dusk_forge_contract::contract;

/// Declares the topics under which an event type is emitted.
///
/// Event authors implement this trait once per event struct, then list the
/// types in the `#[contract(events = [...])]` module attribute. The macro
/// reads [`TOPICS`](ContractEvent::TOPICS) at compile time to populate the
/// contract schema and to dispatch `decode_event` in the data driver.
///
/// A single struct may carry multiple topics — the topic conveys the
/// operation while the struct carries the data — so `TOPICS` is a slice.
///
/// # Example
///
/// ```
/// use dusk_forge::ContractEvent;
///
/// struct OwnershipTransferred;
///
/// impl ContractEvent for OwnershipTransferred {
///     const TOPICS: &'static [&'static str] =
///         &["ownership_transferred", "ownership_renounced"];
/// }
/// ```
pub trait ContractEvent {
    /// The topics under which this event type is emitted.
    const TOPICS: &'static [&'static str];
}
