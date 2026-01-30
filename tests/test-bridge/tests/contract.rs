// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Tests for the test-bridge contract.
//!
//! These tests verify that the `#[contract]` macro correctly generates:
//! - Extern wrappers that can be called via the VM
//! - Trait method exposure via `#[contract(expose = [...])]`
//! - Event emissions

extern crate alloc;

use std::sync::mpsc;
use std::sync::LazyLock;

use dusk_core::abi::ContractId;
use dusk_core::dusk;
use dusk_core::signatures::bls::{PublicKey as AccountPublicKey, SecretKey as AccountSecretKey};
use dusk_vm::CallReceipt;
mod test_session;

use test_session::TestSession;

use types::Address as DSAddress;
use types::{
    EVMAddress, PendingWithdrawal, SetEVMAddressOrOffset, WithdrawalId, WithdrawalRequest,
};

use rand::rngs::StdRng;
use rand::SeedableRng;

const DEPLOYER: [u8; 64] = [0u8; 64];

const TEST_BRIDGE_BYTECODE: &[u8] =
    include_bytes!("../../../target/contract/wasm32-unknown-unknown/release/test_bridge.wasm");
pub const TEST_BRIDGE_ID: ContractId = ContractId::from_bytes([1; 32]);

pub const INITIAL_DUSK_BALANCE: u64 = dusk(1_000.0);

// Owner test key-pair
pub static OWNER_SK: LazyLock<AccountSecretKey> = LazyLock::new(|| {
    let mut rng = StdRng::seed_from_u64(0x5EAF00D);
    AccountSecretKey::random(&mut rng)
});
pub static OWNER_PK: LazyLock<AccountPublicKey> =
    LazyLock::new(|| AccountPublicKey::from(&*OWNER_SK));
pub static OWNER_ADDRESS: LazyLock<DSAddress> = LazyLock::new(|| DSAddress::from(*OWNER_PK));

// Other test key-pair
pub static TEST_SK: LazyLock<AccountSecretKey> = LazyLock::new(|| {
    let mut rng = StdRng::seed_from_u64(0xF0CACC1A);
    AccountSecretKey::random(&mut rng)
});
pub static TEST_PK: LazyLock<AccountPublicKey> =
    LazyLock::new(|| AccountPublicKey::from(&*TEST_SK));
pub static TEST_ADDRESS: LazyLock<DSAddress> = LazyLock::new(|| DSAddress::from(*TEST_PK));

struct TestBridgeSession {
    session: TestSession,
}

impl TestBridgeSession {
    fn new() -> Self {
        let mut session = TestSession::instantiate(
            vec![
                (&*OWNER_PK, INITIAL_DUSK_BALANCE),
                (&*TEST_PK, INITIAL_DUSK_BALANCE),
            ],
            vec![],
        );

        session
            .deploy(
                TEST_BRIDGE_BYTECODE,
                dusk_vm::ContractData::builder()
                    .owner(DEPLOYER)
                    .init_arg(&(*OWNER_ADDRESS,))
                    .contract_id(TEST_BRIDGE_ID),
            )
            .expect("Deploying test-bridge should succeed");

        Self { session }
    }

    // Contract getters

    fn is_paused(&mut self) -> bool {
        self.session
            .direct_call::<_, bool>(TEST_BRIDGE_ID, "is_paused", &())
            .expect("is_paused should succeed")
            .data
    }

    fn finalization_period(&mut self) -> u64 {
        self.session
            .direct_call::<_, u64>(TEST_BRIDGE_ID, "finalization_period", &())
            .expect("finalization_period should succeed")
            .data
    }

    fn other_bridge(&mut self) -> EVMAddress {
        self.session
            .direct_call::<_, EVMAddress>(TEST_BRIDGE_ID, "other_bridge", &())
            .expect("other_bridge should succeed")
            .data
    }

    // OwnableUpgradeable trait methods

    fn owner(&mut self) -> Option<DSAddress> {
        self.session
            .direct_call::<_, Option<DSAddress>>(TEST_BRIDGE_ID, "owner", &())
            .expect("owner should succeed")
            .data
    }

    fn transfer_ownership(
        &mut self,
        sender_sk: &AccountSecretKey,
        new_owner: DSAddress,
    ) -> CallReceipt<()> {
        self.session
            .call_public(sender_sk, TEST_BRIDGE_ID, "transfer_ownership", &new_owner)
            .expect("transfer_ownership should succeed")
    }

    fn renounce_ownership(&mut self, sender_sk: &AccountSecretKey) -> CallReceipt<()> {
        self.session
            .call_public(sender_sk, TEST_BRIDGE_ID, "renounce_ownership", &())
            .expect("renounce_ownership should succeed")
    }

    // Mutating methods

    fn pause(&mut self, sender_sk: &AccountSecretKey) -> CallReceipt<()> {
        self.session
            .call_public(sender_sk, TEST_BRIDGE_ID, "pause", &())
            .expect("pause should succeed")
    }

    fn unpause(&mut self, sender_sk: &AccountSecretKey) -> CallReceipt<()> {
        self.session
            .call_public(sender_sk, TEST_BRIDGE_ID, "unpause", &())
            .expect("unpause should succeed")
    }

    fn set_evm_address_or_offset(
        &mut self,
        sender_sk: &AccountSecretKey,
        value: SetEVMAddressOrOffset,
    ) -> CallReceipt<()> {
        self.session
            .call_public(
                sender_sk,
                TEST_BRIDGE_ID,
                "set_evm_address_or_offset",
                &value,
            )
            .expect("set_evm_address_or_offset should succeed")
    }

    fn other_bridge_ref(&mut self) -> EVMAddress {
        self.session
            .direct_call::<_, EVMAddress>(TEST_BRIDGE_ID, "other_bridge_ref", &())
            .expect("other_bridge_ref should succeed")
            .data
    }

    fn verify_withdrawal(&mut self, withdrawal: PendingWithdrawal) -> bool {
        self.session
            .direct_call::<_, bool>(TEST_BRIDGE_ID, "verify_withdrawal", &withdrawal)
            .expect("verify_withdrawal should succeed")
            .data
    }

    fn initiate_transfer(
        &mut self,
        sender_sk: &AccountSecretKey,
        from: EVMAddress,
        to: DSAddress,
        amount: u64,
    ) -> CallReceipt<()> {
        self.session
            .call_public(
                sender_sk,
                TEST_BRIDGE_ID,
                "initiate_transfer",
                &(from, to, amount),
            )
            .expect("initiate_transfer should succeed")
    }

    fn add_pending_withdrawal(
        &mut self,
        sender_sk: &AccountSecretKey,
        withdrawal: WithdrawalRequest,
    ) -> CallReceipt<()> {
        self.session
            .call_public(
                sender_sk,
                TEST_BRIDGE_ID,
                "add_pending_withdrawal",
                &withdrawal,
            )
            .expect("add_pending_withdrawal should succeed")
    }

    /// Call the pending_withdrawals streaming function and collect all fed tuples.
    fn collect_pending_withdrawals(&mut self) -> Vec<(WithdrawalId, PendingWithdrawal)> {
        let (sender, receiver) = mpsc::channel();

        self.session
            .feeder_call::<_, ()>(TEST_BRIDGE_ID, "pending_withdrawals", &(), sender)
            .expect("pending_withdrawals feeder_call should succeed");

        receiver
            .into_iter()
            .map(|data| test_session::rkyv_deserialize::<(WithdrawalId, PendingWithdrawal)>(&data))
            .collect()
    }

    /// Call the pending_withdrawal_ids streaming function and collect all fed IDs.
    fn collect_pending_withdrawal_ids(&mut self) -> Vec<WithdrawalId> {
        let (sender, receiver) = mpsc::channel();

        self.session
            .feeder_call::<_, ()>(TEST_BRIDGE_ID, "pending_withdrawal_ids", &(), sender)
            .expect("pending_withdrawal_ids feeder_call should succeed");

        receiver
            .into_iter()
            .map(|data| test_session::rkyv_deserialize::<WithdrawalId>(&data))
            .collect()
    }
}

#[test]
fn test_contract_deploys() {
    let mut session = TestBridgeSession::new();

    // Contract should be initialized with owner
    assert_eq!(session.owner(), Some(*OWNER_ADDRESS));
}

#[test]
fn test_inherent_methods() {
    let mut session = TestBridgeSession::new();

    // Test getters
    assert!(!session.is_paused());
    assert_eq!(session.finalization_period(), 100);
    assert_eq!(session.other_bridge(), EVMAddress([0u8; 20]));

    // Test pause/unpause
    session.pause(&OWNER_SK);
    assert!(session.is_paused());

    session.unpause(&OWNER_SK);
    assert!(!session.is_paused());
}

#[test]
fn test_trait_methods_exposed() {
    let mut session = TestBridgeSession::new();

    // owner() should be exposed from OwnableUpgradeable
    assert_eq!(session.owner(), Some(*OWNER_ADDRESS));

    // transfer_ownership() should be exposed
    let receipt = session.transfer_ownership(&OWNER_SK, *TEST_ADDRESS);
    assert_eq!(session.owner(), Some(*TEST_ADDRESS));

    // Check that ownership transfer event was emitted
    assert!(
        !receipt.events.is_empty(),
        "transfer_ownership should emit an event"
    );
}

#[test]
fn test_renounce_ownership() {
    let mut session = TestBridgeSession::new();

    // renounce_ownership() should be exposed
    let receipt = session.renounce_ownership(&OWNER_SK);
    assert_eq!(session.owner(), None);

    // Check that ownership renounced event was emitted
    assert!(
        !receipt.events.is_empty(),
        "renounce_ownership should emit an event"
    );
}

#[test]
fn test_pause_emits_event() {
    let mut session = TestBridgeSession::new();

    let receipt = session.pause(&OWNER_SK);

    // Check that pause event was emitted
    assert!(!receipt.events.is_empty(), "pause should emit an event");
}

#[test]
fn test_method_returning_reference() {
    let mut session = TestBridgeSession::new();

    // Initially the other_bridge is zeroed
    let other_bridge = session.other_bridge_ref();
    assert_eq!(other_bridge, EVMAddress([0u8; 20]));

    // Set a new other_bridge address
    let new_addr = EVMAddress([42u8; 20]);
    session.set_evm_address_or_offset(&OWNER_SK, SetEVMAddressOrOffset::OtherBridge(new_addr));

    // Call the method that returns a reference
    // The macro should have generated .clone() so this works
    let other_bridge = session.other_bridge_ref();
    assert_eq!(other_bridge, new_addr);

    // Verify the regular getter returns the same value
    assert_eq!(session.other_bridge(), other_bridge);
}

#[test]
fn test_method_with_reference_parameter() {
    let mut session = TestBridgeSession::new();

    // Create a valid withdrawal (amount > 0 and block_height > 0)
    // PendingWithdrawal: from is EVMAddress, to is DSAddress
    let valid_withdrawal = PendingWithdrawal {
        from: EVMAddress([1u8; 20]),
        to: *OWNER_ADDRESS,
        amount: 1000,
        block_height: 100,
    };

    // The macro should receive PendingWithdrawal and pass &withdrawal to the method
    let is_valid = session.verify_withdrawal(valid_withdrawal);
    assert!(
        is_valid,
        "withdrawal with amount > 0 and block_height > 0 should be valid"
    );

    // Create an invalid withdrawal (amount = 0)
    let invalid_withdrawal = PendingWithdrawal {
        from: EVMAddress([2u8; 20]),
        to: *OWNER_ADDRESS,
        amount: 0,
        block_height: 100,
    };

    let is_valid = session.verify_withdrawal(invalid_withdrawal);
    assert!(!is_valid, "withdrawal with amount = 0 should be invalid");
}

#[test]
fn test_method_with_multiple_parameters() {
    let mut session = TestBridgeSession::new();

    let from = EVMAddress([1u8; 20]);
    let to = *OWNER_ADDRESS;
    let amount = 5000u64;

    // The macro creates a tuple input type (EVMAddress, DSAddress, u64)
    let receipt = session.initiate_transfer(&OWNER_SK, from, to, amount);

    // Verify event was emitted with correct values
    assert!(
        !receipt.events.is_empty(),
        "initiate_transfer should emit BridgeInitiated event"
    );
}

// =============================================================================
// Trait default implementation tests
// =============================================================================
//
// These tests verify that empty trait method bodies correctly trigger
// the trait's default implementation rather than doing nothing.

#[test]
fn test_trait_default_implementation_emits_event() {
    let mut session = TestBridgeSession::new();

    // The transfer_ownership method has an empty body in the contract:
    //   fn transfer_ownership(&mut self, new_owner: DSAddress) {}
    //
    // The macro should generate code that calls the trait's default:
    //   OwnableUpgradeable::transfer_ownership(&mut STATE, new_owner)
    //
    // The trait's default implementation emits an OwnershipTransferred event.
    // If the macro incorrectly used the empty body, no event would be emitted.

    let receipt = session.transfer_ownership(&OWNER_SK, *TEST_ADDRESS);

    // Verify the trait's default implementation was called by checking:
    // 1. Ownership actually changed
    assert_eq!(
        session.owner(),
        Some(*TEST_ADDRESS),
        "Ownership should have changed - trait default must set new owner"
    );

    // 2. Event was emitted (trait default emits OwnershipTransferred)
    assert!(
        !receipt.events.is_empty(),
        "Trait default should emit OwnershipTransferred event"
    );

    // Find the ownership event
    let ownership_event = receipt
        .events
        .iter()
        .find(|e| e.topic.contains("ownership"));
    assert!(
        ownership_event.is_some(),
        "Should have ownership-related event from trait default"
    );
}

#[test]
fn test_trait_default_only_owner_check() {
    let mut session = TestBridgeSession::new();

    // Verify initial owner
    assert_eq!(session.owner(), Some(*OWNER_ADDRESS));

    // Try to transfer ownership as non-owner (TEST_SK is not the owner)
    // The trait's default implementation should call only_owner() which panics
    let result = session.session.call_public::<_, ()>(
        &TEST_SK,
        TEST_BRIDGE_ID,
        "transfer_ownership",
        &*OWNER_ADDRESS,
    );

    // Should fail because TEST_SK is not the owner
    assert!(
        result.is_err(),
        "Non-owner should not be able to transfer ownership"
    );

    // Verify ownership didn't change
    assert_eq!(
        session.owner(),
        Some(*OWNER_ADDRESS),
        "Ownership should remain unchanged after failed transfer"
    );
}

#[test]
fn test_trait_default_renounce_only_owner() {
    let mut session = TestBridgeSession::new();

    // Try to renounce ownership as non-owner
    let result =
        session
            .session
            .call_public::<_, ()>(&TEST_SK, TEST_BRIDGE_ID, "renounce_ownership", &());

    // Should fail because TEST_SK is not the owner
    assert!(
        result.is_err(),
        "Non-owner should not be able to renounce ownership"
    );

    // Verify ownership didn't change
    assert_eq!(
        session.owner(),
        Some(*OWNER_ADDRESS),
        "Ownership should remain unchanged after failed renounce"
    );
}

// =============================================================================
// Multiple trait implementation tests
// =============================================================================
//
// These tests verify that multiple trait implementations with
// `#[contract(expose = [...])]` are correctly handled.

#[test]
fn test_multiple_trait_implementations() {
    let mut session = TestBridgeSession::new();

    // Test OwnableUpgradeable trait methods (first trait impl)
    assert_eq!(
        session.owner(),
        Some(*OWNER_ADDRESS),
        "owner() from OwnableUpgradeable should work"
    );

    // Test Pausable trait methods (second trait impl)
    // paused() should return the current paused state
    let paused_result = session
        .session
        .direct_call::<_, bool>(TEST_BRIDGE_ID, "paused", &())
        .expect("paused should succeed");
    assert!(
        !paused_result.data,
        "Contract should not be paused initially"
    );

    // toggle_pause() should toggle and return the new state
    let toggle_result = session
        .session
        .call_public::<_, bool>(&OWNER_SK, TEST_BRIDGE_ID, "toggle_pause", &())
        .expect("toggle_pause should succeed");
    assert!(
        toggle_result.data,
        "toggle_pause should return true (now paused)"
    );

    // paused() should now return true
    let paused_result = session
        .session
        .direct_call::<_, bool>(TEST_BRIDGE_ID, "paused", &())
        .expect("paused should succeed");
    assert!(paused_result.data, "Contract should be paused after toggle");

    // Toggle again to unpause
    let toggle_result = session
        .session
        .call_public::<_, bool>(&OWNER_SK, TEST_BRIDGE_ID, "toggle_pause", &())
        .expect("toggle_pause should succeed");
    assert!(
        !toggle_result.data,
        "toggle_pause should return false (now unpaused)"
    );

    // Verify OwnableUpgradeable still works after using Pausable
    assert_eq!(
        session.owner(),
        Some(*OWNER_ADDRESS),
        "owner() should still work"
    );
}

#[test]
fn test_nested_generic_return_type() {
    let mut session = TestBridgeSession::new();

    // Add a pending withdrawal
    let withdrawal = make_withdrawal_request(1, 1000);
    session.add_pending_withdrawal(&OWNER_SK, withdrawal);

    // Call pending_withdrawal_with_id which returns Option<(WithdrawalId, PendingWithdrawal)>
    let id = WithdrawalId([1u8; 32]);
    let result = session
        .session
        .direct_call::<_, Option<(WithdrawalId, PendingWithdrawal)>>(
            TEST_BRIDGE_ID,
            "pending_withdrawal_with_id",
            &id,
        )
        .expect("pending_withdrawal_with_id should succeed");

    // Verify the nested type is returned correctly
    assert!(result.data.is_some(), "Should find the pending withdrawal");
    let (returned_id, returned_pw) = result.data.unwrap();
    assert_eq!(returned_id.0, [1u8; 32], "WithdrawalId should match");
    assert_eq!(
        returned_pw.amount, 1000,
        "PendingWithdrawal amount should match"
    );

    // Test with non-existent ID
    let missing_id = WithdrawalId([99u8; 32]);
    let result = session
        .session
        .direct_call::<_, Option<(WithdrawalId, PendingWithdrawal)>>(
            TEST_BRIDGE_ID,
            "pending_withdrawal_with_id",
            &missing_id,
        )
        .expect("pending_withdrawal_with_id should succeed");

    assert!(
        result.data.is_none(),
        "Should return None for non-existent ID"
    );
}

// =============================================================================
// Streaming function tests (abi::feed integration)
// =============================================================================
//
// These tests verify that functions using `abi::feed()` with the
// `#[contract(feeds = "Type")]` attribute work correctly end-to-end.

/// Helper to create a WithdrawalRequest for testing.
fn make_withdrawal_request(id_byte: u8, amount_lux: u64) -> WithdrawalRequest {
    // Use the WithdrawalRequest::new constructor which properly encodes
    // the destination address in extra_data format
    WithdrawalRequest::new(
        WithdrawalId([id_byte; 32]),
        EVMAddress([id_byte; 20]),
        *OWNER_PK, // destination public key
        amount_lux,
        vec![], // no additional extra_data
    )
}

#[test]
fn test_streaming_function_empty() {
    let mut session = TestBridgeSession::new();

    // Call streaming function with no pending withdrawals
    let results = session.collect_pending_withdrawals();
    assert!(
        results.is_empty(),
        "Should return empty when no pending withdrawals"
    );

    // Same for IDs only
    let ids = session.collect_pending_withdrawal_ids();
    assert!(
        ids.is_empty(),
        "Should return empty IDs when no pending withdrawals"
    );
}

#[test]
fn test_streaming_function_single_withdrawal() {
    let mut session = TestBridgeSession::new();

    // Add a pending withdrawal
    let withdrawal = make_withdrawal_request(1, 1000);
    session.add_pending_withdrawal(&OWNER_SK, withdrawal);

    // Call streaming function - should feed one tuple
    let results = session.collect_pending_withdrawals();
    assert_eq!(
        results.len(),
        1,
        "Should have exactly one pending withdrawal"
    );

    let (id, pending) = &results[0];
    assert_eq!(id.0, [1u8; 32], "WithdrawalId should match");
    assert_eq!(
        pending.from,
        EVMAddress([1u8; 20]),
        "from address should match"
    );
    assert_eq!(
        pending.amount, 1000,
        "amount should match (converted to LUX)"
    );

    // Test IDs only streaming function
    let ids = session.collect_pending_withdrawal_ids();
    assert_eq!(ids.len(), 1, "Should have exactly one ID");
    assert_eq!(ids[0].0, [1u8; 32], "ID should match");
}

#[test]
fn test_streaming_function_multiple_withdrawals() {
    let mut session = TestBridgeSession::new();

    // Add multiple pending withdrawals
    for i in 1..=5u8 {
        let withdrawal = make_withdrawal_request(i, (i as u64) * 1000);
        session.add_pending_withdrawal(&OWNER_SK, withdrawal);
    }

    // Call streaming function - should feed all withdrawals
    let results = session.collect_pending_withdrawals();
    assert_eq!(results.len(), 5, "Should have 5 pending withdrawals");

    // Verify all withdrawals are present (order may vary due to BTreeMap)
    let mut found_ids: Vec<u8> = results.iter().map(|(id, _)| id.0[0]).collect();
    found_ids.sort();
    assert_eq!(
        found_ids,
        vec![1, 2, 3, 4, 5],
        "All withdrawal IDs should be present"
    );

    // Verify amounts match their IDs
    for (id, pending) in &results {
        let expected_amount = (id.0[0] as u64) * 1000;
        assert_eq!(
            pending.amount, expected_amount,
            "Amount for ID {} should be {}",
            id.0[0], expected_amount
        );
    }

    // Test IDs only streaming function
    let ids = session.collect_pending_withdrawal_ids();
    assert_eq!(ids.len(), 5, "Should have 5 IDs");

    let mut id_bytes: Vec<u8> = ids.iter().map(|id| id.0[0]).collect();
    id_bytes.sort();
    assert_eq!(id_bytes, vec![1, 2, 3, 4, 5], "All IDs should be present");
}

#[test]
fn test_streaming_function_after_finalization() {
    let mut session = TestBridgeSession::new();

    // Add withdrawals
    for i in 1..=3u8 {
        let withdrawal = make_withdrawal_request(i, (i as u64) * 1000);
        session.add_pending_withdrawal(&OWNER_SK, withdrawal);
    }

    // Verify we have 3 withdrawals
    let results = session.collect_pending_withdrawals();
    assert_eq!(
        results.len(),
        3,
        "Should have 3 pending withdrawals initially"
    );

    // Finalize one withdrawal (ID = 2)
    let id_to_finalize = WithdrawalId([2u8; 32]);
    session
        .session
        .call_public::<_, ()>(
            &OWNER_SK,
            TEST_BRIDGE_ID,
            "finalize_withdrawal",
            &id_to_finalize,
        )
        .expect("finalize_withdrawal should succeed");

    // Streaming should now return only 2 withdrawals
    let results = session.collect_pending_withdrawals();
    assert_eq!(
        results.len(),
        2,
        "Should have 2 pending withdrawals after finalization"
    );

    // Verify the finalized one is gone
    let remaining_ids: Vec<u8> = results.iter().map(|(id, _)| id.0[0]).collect();
    assert!(
        !remaining_ids.contains(&2),
        "Finalized withdrawal should not be in results"
    );
}
