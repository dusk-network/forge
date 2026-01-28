// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Test contract for the `#[contract]` macro.
//!
//! This contract demonstrates and tests various macro features:
//! - Public method extraction
//! - Event emission detection
//! - Trait implementation exposure via `#[contract(expose = [...])]`
//! - Documentation comment extraction
//! - Custom serialization markers

#![no_std]
#![cfg(target_family = "wasm")]
#![deny(unused_extern_crates)]
#![deny(missing_docs)]
#![deny(clippy::pedantic)]

// Require explicit feature selection for WASM builds
#[cfg(not(any(feature = "contract", feature = "data-driver")))]
compile_error!("Enable either 'contract' or 'data-driver' feature for WASM builds");

extern crate alloc;

/// Test bridge contract demonstrating macro features.
#[dusk_wasm::contract]
mod test_bridge {
    use alloc::collections::BTreeMap;

    use dusk_core::abi;
    use evm_core::standard_bridge::events;
    use evm_core::standard_bridge::{
        Deposit, EVMAddress, PendingWithdrawal, SetEVMAddressOrOffset, SetU64, WithdrawalId,
        WithdrawalRequest,
    };
    use evm_core::{Address as DSAddress, OwnableUpgradeable};

    // =========================================================================
    // Test trait for multiple trait implementation testing
    // =========================================================================

    /// A simple trait for testing multiple trait implementations.
    ///
    /// This trait exists solely to verify that the macro correctly handles
    /// multiple `#[contract(expose = [...])]` trait implementations.
    pub trait Pausable {
        /// Returns whether the contract is currently paused.
        fn paused(&self) -> bool;

        /// Toggles the paused state and returns the new state.
        fn toggle_pause(&mut self) -> bool;
    }

    /// Test bridge contract state.
    ///
    /// Minimal contract struct that mirrors StandardBridge fields
    /// for testing the macro's extraction capabilities.
    pub struct TestBridge {
        /// The contract owner.
        owner: Option<DSAddress>,
        /// Whether the bridge is paused.
        is_paused: bool,
        /// Finalization period in blocks.
        finalization_period: u64,
        /// Address of the other bridge.
        other_bridge: EVMAddress,
        /// Pending withdrawals awaiting finalization.
        pending_withdrawals: BTreeMap<WithdrawalId, PendingWithdrawal>,
    }

    impl TestBridge {
        /// Creates a new empty instance of the test bridge contract state.
        pub const fn new() -> Self {
            Self {
                owner: None,
                is_paused: false,
                finalization_period: 100,
                other_bridge: EVMAddress([0u8; 20]),
                pending_withdrawals: BTreeMap::new(),
            }
        }

        /// Initializes the contract with an owner.
        pub fn init(&mut self, owner: DSAddress) {
            self.owner = Some(owner);
        }

        /// Returns whether the bridge is paused.
        pub fn is_paused(&self) -> bool {
            self.is_paused
        }

        /// Pauses the bridge.
        pub fn pause(&mut self) {
            self.is_paused = true;
            abi::emit(events::PauseToggled::PAUSED, events::PauseToggled());
        }

        /// Unpauses the bridge.
        pub fn unpause(&mut self) {
            self.is_paused = false;
            abi::emit(events::PauseToggled::UNPAUSED, events::PauseToggled());
        }

        /// Returns the finalization period.
        pub fn finalization_period(&self) -> u64 {
            self.finalization_period
        }

        /// Sets a u64 configuration value.
        pub fn set_u64(&mut self, value: SetU64) {
            if let SetU64::FinalizationPeriod(new_value) = value {
                let previous = core::mem::replace(
                    &mut self.finalization_period,
                    new_value,
                );
                abi::emit(
                    events::U64Set::FINALIZATION_PERIOD,
                    events::U64Set { previous, new: new_value },
                );
            }
        }

        /// Sets an EVM address configuration value.
        pub fn set_evm_address_or_offset(&mut self, value: SetEVMAddressOrOffset) {
            if let SetEVMAddressOrOffset::OtherBridge(new_value) = value {
                let previous =
                    core::mem::replace(&mut self.other_bridge, new_value);
                abi::emit(
                    events::EVMAddressOrOffsetSet::OTHER_BRIDGE,
                    events::EVMAddressOrOffsetSet { previous, new: new_value },
                );
            }
        }

        /// Returns the other bridge address.
        pub fn other_bridge(&self) -> EVMAddress {
            self.other_bridge
        }
    }

    // =========================================================================
    // Second impl block - tests that macro handles multiple impl blocks
    // =========================================================================
    //
    // This second impl block verifies the macro correctly merges functions
    // from multiple inherent impl blocks into the schema and extern wrappers.

    impl TestBridge {
        /// Deposits funds.
        pub fn deposit(&mut self, deposit: Deposit) {
            assert!(!self.is_paused, "bridge is paused");

            abi::emit(
                events::TransactionDeposited::TOPIC,
                events::TransactionDeposited {
                    from: EVMAddress::default(),
                    to: deposit.to,
                    version: [0u8; 32],
                    opaque_data: deposit.extra_data.clone(),
                },
            );
            abi::emit(
                events::BridgeInitiated::TOPIC,
                events::BridgeInitiated {
                    from: None,
                    to: deposit.to,
                    amount: deposit.amount,
                    deposit_fee: deposit.fee,
                    extra_data: deposit.extra_data,
                },
            );
        }

        /// Returns a pending withdrawal.
        pub fn pending_withdrawal(&self, id: WithdrawalId) -> Option<PendingWithdrawal> {
            self.pending_withdrawals.get(&id).copied()
        }

        /// Returns a pending withdrawal with its ID as a tuple.
        ///
        /// Tests nested generic types: `Option<(WithdrawalId, PendingWithdrawal)>`.
        /// The schema should correctly capture this nested type structure.
        pub fn pending_withdrawal_with_id(
            &self,
            id: WithdrawalId,
        ) -> Option<(WithdrawalId, PendingWithdrawal)> {
            self.pending_withdrawals.get(&id).map(|pw| (id, *pw))
        }

        /// Returns a reference to the other bridge address.
        ///
        /// Tests that the macro correctly generates `.clone()` for reference returns.
        /// The macro should transform this to `STATE.other_bridge_ref().clone()`.
        pub fn other_bridge_ref(&self) -> &EVMAddress {
            &self.other_bridge
        }

        /// Verifies a pending withdrawal without taking ownership.
        ///
        /// Tests that the macro correctly handles reference parameters by receiving
        /// owned values and passing references. The macro should generate code that
        /// receives `PendingWithdrawal` and passes `&withdrawal` to this method.
        pub fn verify_withdrawal(&self, withdrawal: &PendingWithdrawal) -> bool {
            withdrawal.amount > 0 && withdrawal.block_height > 0
        }

        /// Initiates a bridge transfer with explicit parameters.
        ///
        /// Tests tuple parameter handling - the macro creates a tuple input type
        /// `(EVMAddress, DSAddress, u64)` for the three parameters.
        pub fn initiate_transfer(&mut self, from: EVMAddress, to: DSAddress, amount: u64) {
            assert!(!self.is_paused, "bridge is paused");
            abi::emit(
                events::BridgeInitiated::TOPIC,
                events::BridgeInitiated {
                    from: Some(to),
                    to: from,
                    amount,
                    deposit_fee: 0,
                    extra_data: alloc::vec::Vec::new(),
                },
            );
        }

        /// Adds a pending withdrawal.
        pub fn add_pending_withdrawal(&mut self, withdrawal: WithdrawalRequest) {
            let id = withdrawal.id;
            let pending: PendingWithdrawal =
                withdrawal.try_into().expect("invalid withdrawal request");

            self.pending_withdrawals.insert(id, pending);

            abi::emit(
                events::PendingWithdrawal::ADDED,
                events::PendingWithdrawal {
                    from: pending.from,
                    to: pending.to,
                    amount: pending.amount,
                    block_height: pending.block_height,
                },
            );
        }

        /// Finalizes a withdrawal.
        pub fn finalize_withdrawal(&mut self, id: WithdrawalId) {
            let pending = self
                .pending_withdrawals
                .remove(&id)
                .expect("withdrawal not found");

            abi::emit(
                events::BridgeFinalized::TOPIC,
                events::BridgeFinalized {
                    from: pending.from,
                    to: pending.to,
                    amount: pending.amount,
                },
            );
        }

        // =====================================================================
        // Streaming functions (using abi::feed)
        // =====================================================================
        //
        // These functions demonstrate the `#[contract(feeds = "Type")]` attribute
        // for functions that stream data to the host via `abi::feed()` instead of
        // returning a value directly.

        /// Feeds all pending withdrawals to the host.
        ///
        /// This function streams `(WithdrawalId, PendingWithdrawal)` tuples to the
        /// host one at a time. The `feeds` attribute tells the data-driver what
        /// type to use for decoding the output.
        #[contract(feeds = "(WithdrawalId, PendingWithdrawal)")]
        pub fn pending_withdrawals(&self) {
            for (id, pending) in &self.pending_withdrawals {
                abi::feed((*id, *pending));
            }
        }

        /// Feeds all pending withdrawal IDs to the host.
        ///
        /// This is a simpler example that feeds just the `WithdrawalId`.
        #[contract(feeds = "WithdrawalId")]
        pub fn pending_withdrawal_ids(&self) {
            for id in self.pending_withdrawals.keys() {
                abi::feed(*id);
            }
        }
    }

    /// `OwnableUpgradeable` trait implementation.
    ///
    /// Demonstrates exposing trait methods as contract functions using
    /// `#[contract(expose = [...])]`. Only the listed methods become
    /// contract functions; `owner_mut` and `only_owner` remain internal.
    ///
    /// Note: Empty implementations signal the macro to use trait defaults.
    #[contract(expose = [owner, transfer_ownership, renounce_ownership])]
    // The `#[contract]` macro requires empty method bodies to signal that
    // the trait's default implementations should be used. These empty bodies
    // trigger clippy warnings about unused `self` and pass-by-value parameters,
    // which we suppress here since the pattern is intentional.
    #[allow(clippy::unused_self, clippy::needless_pass_by_value)]
    impl OwnableUpgradeable for TestBridge {
        /// Returns the current owner of the contract.
        fn owner(&self) -> Option<DSAddress> {
            self.owner
        }

        /// Returns a mutable reference to the owner (internal use only).
        fn owner_mut(&mut self) -> &mut Option<DSAddress> {
            &mut self.owner
        }

        /// Transfers ownership to a new address.
        /// Empty body signals the macro to use the trait's default implementation.
        fn transfer_ownership(&mut self, new_owner: DSAddress) {}

        /// Renounces ownership of the contract.
        /// Empty body signals the macro to use the trait's default implementation.
        fn renounce_ownership(&mut self) {}
    }

    /// `Pausable` trait implementation.
    ///
    /// Demonstrates that the macro correctly handles multiple trait implementations
    /// with `#[contract(expose = [...])]`. Both `OwnableUpgradeable` and `Pausable`
    /// methods should appear in the schema and be callable.
    #[contract(expose = [paused, toggle_pause])]
    impl Pausable for TestBridge {
        /// Returns whether the contract is currently paused.
        fn paused(&self) -> bool {
            self.is_paused
        }

        /// Toggles the paused state and returns the new state.
        fn toggle_pause(&mut self) -> bool {
            self.is_paused = !self.is_paused;
            self.is_paused
        }
    }

    // =========================================================================
    // Custom data-driver functions
    // =========================================================================
    //
    // These functions are NOT contract methods - they exist only in the data-driver
    // to provide encoding/decoding utilities for external tools (e.g., web wallets).
    //
    // IMPORTANT: These functions are moved into the `data_driver` module during
    // macro expansion, so they must use fully-qualified paths for types.

    /// Custom encoder for the `extra_data` data-driver function.
    ///
    /// This demonstrates custom data-driver functions that only exist in the
    /// data-driver, not as actual contract methods. This is useful for providing
    /// encoding/decoding utilities to external tools.
    #[contract(encode_input = "extra_data")]
    fn encode_extra_data(json: &str) -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error> {
        // Parse the input JSON as an EVMAddress and return its bytes
        let addr: evm_core::standard_bridge::EVMAddress = serde_json::from_str(json)?;
        Ok(addr.0.to_vec())
    }

    /// Custom decoder for the `extra_data` data-driver function.
    #[contract(decode_output = "extra_data")]
    fn decode_extra_data(rkyv: &[u8]) -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error> {
        // Decode the bytes as an EVMAddress and return as JSON
        if rkyv.len() != 20 {
            return Err(dusk_data_driver::Error::Unsupported(
                alloc::format!("expected 20 bytes, got {}", rkyv.len()),
            ));
        }
        let mut addr = [0u8; 20];
        addr.copy_from_slice(rkyv);
        let evm_addr = evm_core::standard_bridge::EVMAddress(addr);
        Ok(serde_json::to_value(evm_addr)?)
    }
}
