// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Test contract for the `#[contract]` macro.
//!
//! Each method exists to exercise a specific macro code path:
//! - Simple value return, reference return, Option return, nested generics
//! - Single/multi param inputs, struct params, reference params
//! - Event emission, streaming via `abi::feed`
//! - Trait exposure with default implementations
//! - Multiple impl blocks, associated functions
//! - Custom data-driver encode/decode

#![no_std]
#![cfg(target_family = "wasm")]
#![deny(unused_extern_crates)]
#![deny(missing_docs)]
#![deny(clippy::pedantic)]

/// Test contract demonstrating all macro features.
#[dusk_forge::contract]
mod test_contract {
    extern crate alloc;

    use alloc::collections::BTreeMap;
    use alloc::string::String;

    use dusk_core::abi;
    use dusk_core::signatures::bls::PublicKey;
    use types::events;
    use types::{Item, ItemId, Ownable};

    // =========================================================================
    // Versioned trait — tests trait-exposed associated function (no self)
    // =========================================================================

    /// A trait for querying the contract version.
    pub trait Versioned {
        /// Returns the contract version string.
        fn version() -> String {
            String::from(env!("CARGO_PKG_VERSION"))
        }
    }

    /// Test contract state.
    ///
    /// Designed to exercise every `#[contract]` macro code path.
    pub struct TestContract {
        /// The contract owner.
        owner: Option<PublicKey>,
        /// A simple counter for scalar get/set testing.
        counter: u64,
        /// A label for reference return testing.
        label: String,
        /// A collection of items for streaming and lookup testing.
        items: BTreeMap<ItemId, Item>,
    }

    // =========================================================================
    // First impl block
    // =========================================================================

    impl TestContract {
        /// Creates a new empty instance of the test contract.
        pub const fn new() -> Self {
            Self {
                owner: None,
                counter: 0,
                label: String::new(),
                items: BTreeMap::new(),
            }
        }

        /// Initializes the contract with an owner.
        pub fn init(&mut self, owner: PublicKey) {
            self.owner = Some(owner);
        }

        /// Returns the current counter value.
        ///
        /// Exercises: simple scalar return.
        pub fn counter(&self) -> u64 {
            self.counter
        }

        /// Returns a reference to the label.
        ///
        /// Exercises: reference return (macro must generate `.clone()`).
        pub fn label(&self) -> &String {
            &self.label
        }

        /// Sets the counter to a new value.
        ///
        /// Exercises: single parameter setter + event emission.
        pub fn set_counter(&mut self, value: u64) {
            let previous = core::mem::replace(&mut self.counter, value);
            abi::emit(
                events::CounterUpdated::TOPIC,
                events::CounterUpdated {
                    previous,
                    new: value,
                },
            );
        }

        /// Updates both counter and label.
        ///
        /// Exercises: multi-parameter input (macro generates tuple) + event.
        pub fn update(&mut self, counter: u64, label: String) {
            self.counter = counter;
            self.label.clone_from(&label);
            abi::emit(
                events::ContractUpdated::TOPIC,
                events::ContractUpdated { counter, label },
            );
        }

        /// Resets the counter to zero.
        ///
        /// Exercises: event emission with unit struct event.
        pub fn reset_counter(&mut self) {
            self.counter = 0;
            abi::emit(events::CounterReset::TOPIC, events::CounterReset());
        }

        /// Returns whether the collection is non-empty.
        ///
        /// Exercises: zero-argument bool return.
        pub fn has_items(&self) -> bool {
            !self.items.is_empty()
        }

        /// Returns a zero `ItemId`.
        ///
        /// Exercises: associated function (no self).
        pub fn empty_id() -> ItemId {
            ItemId(0)
        }
    }

    // =========================================================================
    // Second impl block — tests multiple impl block merging
    // =========================================================================

    impl TestContract {
        /// Adds an item to the collection.
        ///
        /// Exercises: struct parameter + event emission.
        pub fn add_item(&mut self, item: Item) {
            self.items.insert(item.id, item);
            abi::emit(events::Item::ADDED, Item { ..item });
        }

        /// Returns an item by its ID.
        ///
        /// Exercises: Option return type.
        pub fn get_item(&self, id: ItemId) -> Option<Item> {
            self.items.get(&id).copied()
        }

        /// Returns an item with its ID as a tuple.
        ///
        /// Exercises: nested generic return `Option<(ItemId, Item)>`.
        pub fn find_item(&self, id: ItemId) -> Option<(ItemId, Item)> {
            self.items.get(&id).map(|item| (id, *item))
        }

        /// Checks whether an item exists in the collection.
        ///
        /// Exercises: reference parameter (macro receives owned value, passes `&item`).
        pub fn contains_item(&self, item: &Item) -> bool {
            self.items.contains_key(&item.id)
        }

        /// Removes an item from the collection.
        pub fn remove_item(&mut self, id: ItemId) {
            let removed = self.items.remove(&id).expect("item not found");
            abi::emit(events::Item::REMOVED, Item { ..removed });
        }

        // =====================================================================
        // Streaming functions (using abi::feed)
        // =====================================================================

        /// Feeds all items to the host as `(ItemId, Item)` tuples.
        ///
        /// Exercises: `#[contract(feeds = "...")]` with tuple feed type.
        #[contract(feeds = "(ItemId, Item)")]
        pub fn items(&self) {
            for (id, item) in &self.items {
                abi::feed((*id, *item));
            }
        }

        /// Feeds all item IDs to the host.
        ///
        /// Exercises: `#[contract(feeds = "...")]` with simple feed type.
        #[contract(feeds = "ItemId")]
        pub fn item_ids(&self) {
            for id in self.items.keys() {
                abi::feed(*id);
            }
        }
    }

    // =========================================================================
    // Ownable trait — tests trait exposure + default implementations
    // =========================================================================

    /// Demonstrates exposing trait methods as contract functions using
    /// `#[contract(expose = [...])]`. Only the listed methods become
    /// contract functions; `owner_mut` and `only_owner` remain internal.
    ///
    /// Empty method bodies signal the macro to use the trait's default
    /// implementations.
    #[contract(expose = [owner, transfer_ownership, renounce_ownership])]
    #[allow(clippy::unused_self, clippy::needless_pass_by_value)]
    impl Ownable for TestContract {
        /// Returns the current owner of the contract.
        fn owner(&self) -> Option<PublicKey> {
            self.owner
        }

        /// Returns a mutable reference to the owner (internal use only).
        fn owner_mut(&mut self) -> &mut Option<PublicKey> {
            &mut self.owner
        }

        /// Transfers ownership to a new public key.
        /// Empty body signals the macro to use the trait's default implementation.
        fn transfer_ownership(&mut self, new_owner: PublicKey) {}

        /// Renounces ownership of the contract.
        /// Empty body signals the macro to use the trait's default implementation.
        fn renounce_ownership(&mut self) {}
    }

    // =========================================================================
    // Versioned trait — tests multiple trait exposure + associated functions
    // =========================================================================

    /// Demonstrates a second trait with `#[contract(expose = [...])]`.
    /// Also tests associated functions (no `&self`) exposed from traits.
    #[contract(expose = [version])]
    impl Versioned for TestContract {
        /// Returns the contract version.
        /// Empty body signals the macro to use the trait's default implementation.
        fn version() -> String {}
    }

    // =========================================================================
    // Custom data-driver functions
    // =========================================================================

    /// Custom encoder for the `raw_id` data-driver function.
    ///
    /// Demonstrates custom data-driver functions that exist only in the
    /// data-driver WASM, not as contract methods.
    #[contract(encode_input = "raw_id")]
    fn encode_raw_id(json: &str) -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error> {
        let id: u64 = serde_json::from_str(json)?;
        Ok(id.to_le_bytes().to_vec())
    }

    /// Custom decoder for the `raw_id` data-driver function.
    #[contract(decode_output = "raw_id")]
    fn decode_raw_id(bytes: &[u8]) -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error> {
        if bytes.len() != 8 {
            return Err(dusk_data_driver::Error::Unsupported(alloc::format!(
                "expected 8 bytes, got {}",
                bytes.len()
            )));
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(bytes);
        let id = u64::from_le_bytes(buf);
        Ok(serde_json::to_value(id)?)
    }
}
