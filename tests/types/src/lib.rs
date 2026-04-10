// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Types used by the test contract.

#![no_std]
#![deny(missing_docs)]
#![deny(clippy::pedantic)]

extern crate alloc;

use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize, Serialize};

// =========================================================================
// ItemId
// =========================================================================

/// A unique identifier for an item in the contract's collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ItemId(pub u64);

// =========================================================================
// Item
// =========================================================================

/// A data record stored in the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Item {
    /// The item's unique identifier.
    pub id: ItemId,
    /// A numeric value associated with the item.
    pub value: u64,
    /// Whether the item is currently active.
    pub active: bool,
}

// =========================================================================
// Ownable trait
// =========================================================================

#[cfg(feature = "abi")]
use dusk_core::signatures::bls::PublicKey;

/// Trait for contracts with transferable ownership.
#[cfg(feature = "abi")]
pub trait Ownable {
    /// Returns the current owner of the contract.
    fn owner(&self) -> Option<PublicKey>;

    /// Returns a mutable reference to the owner field.
    fn owner_mut(&mut self) -> &mut Option<PublicKey>;

    /// Transfers ownership to a new public key.
    fn transfer_ownership(&mut self, new_owner: PublicKey) {
        use dusk_core::abi;
        self.only_owner();

        let previous_owner = self
            .owner_mut()
            .replace(new_owner)
            .expect(error::INVALID_OWNER);

        abi::emit(
            events::OwnershipTransferred::TRANSFERRED,
            events::OwnershipTransferred {
                previous_owner,
                new_owner: Some(new_owner),
            },
        );
    }

    /// Renounces ownership of the contract.
    fn renounce_ownership(&mut self) {
        use dusk_core::abi;
        self.only_owner();

        let previous_owner = core::mem::take(self.owner_mut()).expect(error::INVALID_OWNER);

        abi::emit(
            events::OwnershipTransferred::RENOUNCED,
            events::OwnershipTransferred {
                previous_owner,
                new_owner: None,
            },
        );
    }

    /// Panics if the caller is not the owner.
    fn only_owner(&self) {
        let sender = dusk_core::abi::public_sender().expect(error::NO_SENDER);
        let current_owner = self.owner().expect(error::INVALID_OWNER);
        assert!(sender == current_owner, "{}", error::UNAUTHORIZED);
    }
}

// =========================================================================
// Events module
// =========================================================================

/// Events emitted by the test contract.
pub mod events {
    use dusk_core::signatures::bls::PublicKey;

    #[allow(unused_imports)]
    use rkyv::bytecheck::CheckBytes;
    use rkyv::{Archive, Deserialize, Serialize};

    /// Event emitted when the counter is reset to zero.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
    #[archive_attr(derive(CheckBytes))]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct CounterReset();

    impl CounterReset {
        /// Event topic for resetting the counter.
        pub const TOPIC: &'static str = "counter_reset";
    }

    /// Event emitted when the counter value changes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
    #[archive_attr(derive(CheckBytes))]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct CounterUpdated {
        /// The previous counter value.
        pub previous: u64,
        /// The new counter value.
        pub new: u64,
    }

    impl CounterUpdated {
        /// Event topic for counter updates.
        pub const TOPIC: &'static str = "counter_updated";
    }

    /// Event emitted when ownership is transferred or renounced.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
    #[archive_attr(derive(CheckBytes))]
    pub struct OwnershipTransferred {
        /// The previous owner.
        pub previous_owner: PublicKey,
        /// The new owner, or `None` if ownership was renounced.
        pub new_owner: Option<PublicKey>,
    }

    impl OwnershipTransferred {
        /// Event topic for ownership transfer.
        pub const TRANSFERRED: &'static str = "ownership_transferred";
        /// Event topic for ownership renunciation.
        pub const RENOUNCED: &'static str = "ownership_renounced";
    }

    // Re-use Item as an event type for item operations.
    pub use super::Item;

    impl Item {
        /// Event topic for adding an item.
        pub const ADDED: &'static str = "item_added";
        /// Event topic for removing an item.
        pub const REMOVED: &'static str = "item_removed";
    }

    /// Event emitted when the contract is updated with new counter and label.
    #[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
    #[archive_attr(derive(CheckBytes))]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct ContractUpdated {
        /// The new counter value.
        pub counter: u64,
        /// The new label.
        pub label: alloc::string::String,
    }

    impl ContractUpdated {
        /// Event topic for contract updates.
        pub const TOPIC: &'static str = "contract_updated";
    }
}

// =========================================================================
// Error constants
// =========================================================================

/// Error constants.
pub mod error {
    /// Error thrown when there is no public sender.
    pub const NO_SENDER: &str = "No public sender available.";

    /// Error thrown when the caller is not the owner.
    pub const UNAUTHORIZED: &str =
        "The caller account is not authorized to perform this operation.";

    /// Error thrown when the owner is not set.
    pub const INVALID_OWNER: &str = "The owner is not a valid owner account, e.g. `None`.";
}
