// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Test the `#[contract]` macro using types from `evm-core`.
//!
//! This test verifies that the macro correctly extracts:
//! - Public method signatures
//! - Event emissions (topic paths and data types)
//! - Documentation comments
//! - Custom attributes

#![allow(dead_code, unused_variables)]

use dusk_wasm::contract;

// Import actual types from evm-core (same types used by StandardBridge)
use evm_core::standard_bridge::events;
use evm_core::standard_bridge::{
    Deposit, EVMAddress, PendingWithdrawal, SetEVMAddressOrOffset, SetU64,
    WithdrawalId, WithdrawalRequest,
};
use evm_core::Address as DSAddress;

// Mock abi module - the real one comes from dusk_core at runtime
mod abi {
    pub fn emit<T>(_topic: &str, _data: T) {}
}

/// Minimal contract struct for testing the macro.
/// Uses the same field types as the real StandardBridge.
struct TestBridge {
    owner: Option<DSAddress>,
    is_paused: bool,
    finalization_period: u64,
    other_bridge: EVMAddress,
}

#[contract]
impl TestBridge {
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
            events::U64Set { previous: 0, new: 0 },
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
                to: DSAddress::Contract(dusk_core::abi::ContractId::from_bytes([0u8; 32])),
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
                to: DSAddress::Contract(dusk_core::abi::ContractId::from_bytes([0u8; 32])),
                amount: 0,
            },
        );
    }

    /// Returns the owner.
    pub fn owner(&self) -> Option<DSAddress> {
        self.owner
    }

    // Private helper - should NOT be exported
    fn only_owner(&self) {}
}

#[test]
fn test_schema_contract_name() {
    assert_eq!(CONTRACT_SCHEMA.name, "TestBridge");
}

#[test]
fn test_public_functions_extracted() {
    let names: Vec<_> = CONTRACT_SCHEMA.iter_functions().map(|f| f.name).collect();

    // All public functions should be present
    assert!(names.contains(&"init"));
    assert!(names.contains(&"is_paused"));
    assert!(names.contains(&"pause"));
    assert!(names.contains(&"unpause"));
    assert!(names.contains(&"finalization_period"));
    assert!(names.contains(&"set_u64"));
    assert!(names.contains(&"set_evm_address_or_offset"));
    assert!(names.contains(&"other_bridge"));
    assert!(names.contains(&"deposit"));
    assert!(names.contains(&"pending_withdrawal"));
    assert!(names.contains(&"add_pending_withdrawal"));
    assert!(names.contains(&"finalize_withdrawal"));
    assert!(names.contains(&"owner"));

    // Private functions should NOT be present
    assert!(!names.contains(&"only_owner"));
}

#[test]
fn test_function_signatures() {
    let init = CONTRACT_SCHEMA.get_function("init").unwrap();
    assert_eq!(init.doc, "Initializes the contract with an owner.");
    assert!(init.input.contains("Address")); // DSAddress is evm_core::Address

    let is_paused = CONTRACT_SCHEMA.get_function("is_paused").unwrap();
    assert!(is_paused.input.contains("()"));
    assert!(is_paused.output.contains("bool"));

    let pending = CONTRACT_SCHEMA.get_function("pending_withdrawal").unwrap();
    assert!(pending.input.contains("WithdrawalId"));
    assert!(pending.output.contains("Option"));
    assert!(pending.output.contains("PendingWithdrawal"));
}

#[test]
fn test_events_extracted() {
    let topics: Vec<_> = CONTRACT_SCHEMA.iter_events().map(|e| e.topic).collect();

    // Event topics are stored as path expressions
    assert!(topics.contains(&"events::PauseToggled::PAUSED"));
    assert!(topics.contains(&"events::PauseToggled::UNPAUSED"));
    assert!(topics.contains(&"events::U64Set::FINALIZATION_PERIOD"));
    assert!(topics.contains(&"events::EVMAddressOrOffsetSet::OTHER_BRIDGE"));
    assert!(topics.contains(&"events::TransactionDeposited::TOPIC"));
    assert!(topics.contains(&"events::BridgeInitiated::TOPIC"));
    assert!(topics.contains(&"events::PendingWithdrawal::ADDED"));
    assert!(topics.contains(&"events::BridgeFinalized::TOPIC"));
}

#[test]
fn test_event_data_types() {
    let paused = CONTRACT_SCHEMA.get_event("events::PauseToggled::PAUSED").unwrap();
    assert!(paused.data.contains("PauseToggled"));

    let deposited = CONTRACT_SCHEMA.get_event("events::TransactionDeposited::TOPIC").unwrap();
    assert!(deposited.data.contains("TransactionDeposited"));

    let finalized = CONTRACT_SCHEMA.get_event("events::BridgeFinalized::TOPIC").unwrap();
    assert!(finalized.data.contains("BridgeFinalized"));
}

#[test]
fn test_schema_matches_expected_json() {
    let expected = include_str!("assets/test_bridge_schema.json");
    let expected: serde_json::Value = serde_json::from_str(expected).unwrap();

    let actual = serde_json::to_value(CONTRACT_SCHEMA).unwrap();

    assert_eq!(expected, actual);
}

#[test]
#[ignore]
fn print_schema_json() {
    let json = serde_json::to_string_pretty(&CONTRACT_SCHEMA).unwrap();
    println!("{json}");
}
