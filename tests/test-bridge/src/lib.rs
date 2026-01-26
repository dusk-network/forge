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

extern crate alloc;

/// Test bridge contract demonstrating macro features.
#[dusk_wasm::contract]
mod test_bridge {
    use alloc::vec::Vec;

    use dusk_core::abi::{self, ContractId};
    use evm_core::standard_bridge::events;
    use evm_core::standard_bridge::{
        Deposit, EVMAddress, PendingWithdrawal, SetEVMAddressOrOffset, SetU64, WithdrawalId,
        WithdrawalRequest,
    };
    use evm_core::{Address as DSAddress, OwnableUpgradeable};

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
    }

    impl TestBridge {
        /// Creates a new empty instance of the test bridge contract state.
        pub const fn new() -> Self {
            Self {
                owner: None,
                is_paused: false,
                finalization_period: 100,
                other_bridge: EVMAddress([0u8; 20]),
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
        pub fn set_u64(&mut self, _value: SetU64) {
            abi::emit(
                events::U64Set::FINALIZATION_PERIOD,
                events::U64Set {
                    previous: 0,
                    new: 0,
                },
            );
        }

        /// Sets an EVM address configuration value.
        pub fn set_evm_address_or_offset(&mut self, _value: SetEVMAddressOrOffset) {
            abi::emit(
                events::EVMAddressOrOffsetSet::OTHER_BRIDGE,
                events::EVMAddressOrOffsetSet {
                    previous: EVMAddress::default(),
                    new: EVMAddress::default(),
                },
            );
        }

        /// Returns the other bridge address.
        pub fn other_bridge(&self) -> EVMAddress {
            self.other_bridge
        }

        /// Deposits funds.
        pub fn deposit(&mut self, _deposit: Deposit) {
            abi::emit(
                events::TransactionDeposited::TOPIC,
                events::TransactionDeposited {
                    from: EVMAddress::default(),
                    to: EVMAddress::default(),
                    version: [0u8; 32],
                    opaque_data: Vec::new(),
                },
            );
            abi::emit(
                events::BridgeInitiated::TOPIC,
                events::BridgeInitiated {
                    from: None,
                    to: EVMAddress::default(),
                    amount: 0,
                    deposit_fee: 0,
                    extra_data: Vec::new(),
                },
            );
        }

        /// Returns a pending withdrawal.
        pub fn pending_withdrawal(&self, _id: WithdrawalId) -> Option<PendingWithdrawal> {
            None
        }

        /// Adds a pending withdrawal.
        pub fn add_pending_withdrawal(&mut self, _withdrawal: WithdrawalRequest) {
            abi::emit(
                events::PendingWithdrawal::ADDED,
                events::PendingWithdrawal {
                    from: EVMAddress::default(),
                    to: DSAddress::Contract(ContractId::from_bytes([0u8; 32])),
                    amount: 0,
                    block_height: 0,
                },
            );
        }

        /// Finalizes a withdrawal.
        pub fn finalize_withdrawal(&mut self, _id: WithdrawalId) {
            abi::emit(
                events::BridgeFinalized::TOPIC,
                events::BridgeFinalized {
                    from: EVMAddress::default(),
                    to: DSAddress::Contract(ContractId::from_bytes([0u8; 32])),
                    amount: 0,
                },
            );
        }

        // Private helper - should NOT be exported
        fn only_owner(&self) {}
    }

    /// OwnableUpgradeable trait implementation.
    ///
    /// Demonstrates exposing trait methods as contract functions using
    /// `#[contract(expose = [...])]`. Only the listed methods become
    /// contract functions; `owner_mut` and `only_owner` remain internal.
    #[contract(expose = [owner, transfer_ownership, renounce_ownership])]
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
        /// Uses the trait's default implementation which emits an event.
        fn transfer_ownership(&mut self, new_owner: DSAddress) {}

        /// Renounces ownership of the contract.
        /// Uses the trait's default implementation which emits an event.
        fn renounce_ownership(&mut self) {}
    }
}
