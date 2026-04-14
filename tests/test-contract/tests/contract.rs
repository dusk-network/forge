// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Tests for the test contract.
//!
//! These tests verify that the `#[contract]` macro correctly generates:
//! - Extern wrappers that can be called via the VM
//! - Trait method exposure via `#[contract(expose = [...])]`
//! - Event emissions

extern crate alloc;

use std::sync::{LazyLock, mpsc};

use dusk_core::abi::{ContractError, ContractId, StandardBufSerializer};
use dusk_core::dusk;
use dusk_core::signatures::bls::{PublicKey as AccountPublicKey, SecretKey as AccountSecretKey};
use dusk_vm::{CallReceipt, Error as VMError};
use rkyv::bytecheck::CheckBytes;
use rkyv::validation::validators::DefaultValidator;
use rkyv::{Archive, Deserialize, Infallible, Serialize};
mod test_session;

use rand::SeedableRng;
use rand::rngs::StdRng;
use test_session::TestSession;
use types::{Item, ItemId};

/// Direct/feeder call helpers used only by this test binary.
///
/// Lives here (not in `test_session.rs`) so the schema test binary, which
/// only needs the public-call path, doesn't trip a `dead_code` warning.
impl TestSession {
    /// Directly calls the contract, circumventing the transfer contract and
    /// (among other things) also any gas-payment.
    fn direct_call<A, R>(
        &mut self,
        contract: ContractId,
        fn_name: &str,
        fn_arg: &A,
    ) -> Result<CallReceipt<R>, ContractError>
    where
        A: for<'b> Serialize<StandardBufSerializer<'b>>,
        A::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
        R: Archive,
        R::Archived: Deserialize<R, Infallible> + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        self.0
            .call::<_, R>(contract, fn_name, fn_arg, u64::MAX)
            .map_err(|e| match e {
                VMError::Panic(panic_msg) => ContractError::Panic(panic_msg),
                VMError::OutOfGas => ContractError::OutOfGas,
                _ => panic!("Unknown error: {e}"),
            })
    }

    /// Feeder calls let the contract report larger amounts of data to the
    /// host via the channel included in this call.
    fn feeder_call<A, R>(
        &mut self,
        contract: ContractId,
        fn_name: &str,
        fn_arg: &A,
        feeder: std::sync::mpsc::Sender<Vec<u8>>,
    ) -> Result<CallReceipt<R>, ContractError>
    where
        A: for<'b> Serialize<StandardBufSerializer<'b>>,
        A::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
        R: Archive,
        R::Archived: Deserialize<R, Infallible> + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        self.0
            .feeder_call::<_, R>(contract, fn_name, fn_arg, u64::MAX, feeder)
            .map_err(|e| match e {
                VMError::Panic(panic_msg) => ContractError::Panic(panic_msg),
                VMError::OutOfGas => ContractError::OutOfGas,
                _ => panic!("Unknown error: {e}"),
            })
    }
}

const DEPLOYER: [u8; 64] = [0u8; 64];

const CONTRACT_BYTECODE: &[u8] =
    include_bytes!("../../../target/contract/wasm32-unknown-unknown/release/test_contract.wasm");
pub const CONTRACT_ID: ContractId = ContractId::from_bytes([1; 32]);

pub const INITIAL_DUSK_BALANCE: u64 = dusk(1_000.0);

// Owner test key-pair
pub static OWNER_SK: LazyLock<AccountSecretKey> = LazyLock::new(|| {
    let mut rng = StdRng::seed_from_u64(0x5EAF00D);
    AccountSecretKey::random(&mut rng)
});
pub static OWNER_PK: LazyLock<AccountPublicKey> =
    LazyLock::new(|| AccountPublicKey::from(&*OWNER_SK));

// Other test key-pair
pub static TEST_SK: LazyLock<AccountSecretKey> = LazyLock::new(|| {
    let mut rng = StdRng::seed_from_u64(0xF0CACC1A);
    AccountSecretKey::random(&mut rng)
});
pub static TEST_PK: LazyLock<AccountPublicKey> =
    LazyLock::new(|| AccountPublicKey::from(&*TEST_SK));

struct TestContractSession {
    session: TestSession,
}

impl TestContractSession {
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
                CONTRACT_BYTECODE,
                dusk_vm::ContractData::builder()
                    .owner(DEPLOYER)
                    .init_arg(&(*OWNER_PK,))
                    .contract_id(CONTRACT_ID),
            )
            .expect("Deploying test contract should succeed");

        Self { session }
    }

    // Simple getters

    fn counter(&mut self) -> u64 {
        self.session
            .direct_call::<_, u64>(CONTRACT_ID, "counter", &())
            .expect("counter should succeed")
            .data
    }

    fn label(&mut self) -> String {
        self.session
            .direct_call::<_, String>(CONTRACT_ID, "label", &())
            .expect("label should succeed")
            .data
    }

    // Ownable trait methods

    fn owner(&mut self) -> Option<AccountPublicKey> {
        self.session
            .direct_call::<_, Option<AccountPublicKey>>(CONTRACT_ID, "owner", &())
            .expect("owner should succeed")
            .data
    }

    fn transfer_ownership(
        &mut self,
        sender_sk: &AccountSecretKey,
        new_owner: AccountPublicKey,
    ) -> CallReceipt<()> {
        self.session
            .call_public(sender_sk, CONTRACT_ID, "transfer_ownership", &new_owner)
            .expect("transfer_ownership should succeed")
    }

    fn renounce_ownership(&mut self, sender_sk: &AccountSecretKey) -> CallReceipt<()> {
        self.session
            .call_public(sender_sk, CONTRACT_ID, "renounce_ownership", &())
            .expect("renounce_ownership should succeed")
    }

    fn bump_tally(&mut self, sender_sk: &AccountSecretKey) -> CallReceipt<()> {
        self.session
            .call_public(sender_sk, CONTRACT_ID, "bump_tally", &())
            .expect("bump_tally should succeed")
    }

    // Mutating methods

    fn set_counter(&mut self, sender_sk: &AccountSecretKey, value: u64) -> CallReceipt<()> {
        self.session
            .call_public(sender_sk, CONTRACT_ID, "set_counter", &value)
            .expect("set_counter should succeed")
    }

    fn update(
        &mut self,
        sender_sk: &AccountSecretKey,
        counter: u64,
        label: String,
    ) -> CallReceipt<()> {
        self.session
            .call_public(sender_sk, CONTRACT_ID, "update", &(counter, label))
            .expect("update should succeed")
    }

    fn reset_counter(&mut self, sender_sk: &AccountSecretKey) -> CallReceipt<()> {
        self.session
            .call_public(sender_sk, CONTRACT_ID, "reset_counter", &())
            .expect("reset_counter should succeed")
    }

    fn add_item(&mut self, sender_sk: &AccountSecretKey, item: Item) -> CallReceipt<()> {
        self.session
            .call_public(sender_sk, CONTRACT_ID, "add_item", &item)
            .expect("add_item should succeed")
    }

    fn contains_item(&mut self, item: Item) -> bool {
        self.session
            .direct_call::<_, bool>(CONTRACT_ID, "contains_item", &item)
            .expect("contains_item should succeed")
            .data
    }

    /// Collect all items via the streaming function.
    fn collect_items(&mut self) -> Vec<(ItemId, Item)> {
        let (sender, receiver) = mpsc::channel();

        self.session
            .feeder_call::<_, ()>(CONTRACT_ID, "items", &(), sender)
            .expect("items feeder_call should succeed");

        receiver
            .into_iter()
            .map(|data| test_session::rkyv_deserialize::<(ItemId, Item)>(&data))
            .collect()
    }

    /// Collect all item IDs via the streaming function.
    fn collect_item_ids(&mut self) -> Vec<ItemId> {
        let (sender, receiver) = mpsc::channel();

        self.session
            .feeder_call::<_, ()>(CONTRACT_ID, "item_ids", &(), sender)
            .expect("item_ids feeder_call should succeed");

        receiver
            .into_iter()
            .map(|data| test_session::rkyv_deserialize::<ItemId>(&data))
            .collect()
    }
}

/// Helper to create a test item.
fn make_item(id: u64, value: u64) -> Item {
    Item {
        id: ItemId(id),
        value,
        active: true,
    }
}

#[test]
fn test_contract_deploys() {
    let mut session = TestContractSession::new();
    assert_eq!(session.owner(), Some(*OWNER_PK));
}

#[test]
fn test_inherent_methods() {
    let mut session = TestContractSession::new();

    // Test initial state
    assert_eq!(session.counter(), 0);
    assert_eq!(session.label(), "");

    // Test set_counter
    session.set_counter(&OWNER_SK, 42);
    assert_eq!(session.counter(), 42);

    // Test reset_counter
    session.reset_counter(&OWNER_SK);
    assert_eq!(session.counter(), 0);
}

#[test]
fn test_trait_methods_exposed() {
    let mut session = TestContractSession::new();

    // owner() should be exposed from Ownable
    assert_eq!(session.owner(), Some(*OWNER_PK));

    // transfer_ownership() should be exposed
    let receipt = session.transfer_ownership(&OWNER_SK, *TEST_PK);
    assert_eq!(session.owner(), Some(*TEST_PK));

    // Check that ownership transfer event was emitted
    assert!(
        !receipt.events.is_empty(),
        "transfer_ownership should emit an event"
    );
}

#[test]
fn test_renounce_ownership() {
    let mut session = TestContractSession::new();

    let receipt = session.renounce_ownership(&OWNER_SK);
    assert_eq!(session.owner(), None);

    assert!(
        !receipt.events.is_empty(),
        "renounce_ownership should emit an event"
    );
}

/// `bump_tally` is an inherent method with `#[contract(emits = [...])]`; the
/// actual `abi::emit` call happens in `emit_tally_bumped`, a free helper
/// outside any contract impl block, so it is invisible to the macro's body
/// scanner. Verify the call still succeeds and the delegated event is
/// emitted at runtime.
#[test]
fn test_delegating_inherent_method_emits_event() {
    let mut session = TestContractSession::new();

    let receipt = session.bump_tally(&OWNER_SK);

    let tally_event = receipt.events.iter().find(|e| e.topic == "tally_bumped");
    assert!(
        tally_event.is_some(),
        "bump_tally should emit tally_bumped via helper; got: {:?}",
        receipt.events
    );
}

#[test]
fn test_set_counter_emits_event() {
    let mut session = TestContractSession::new();

    let receipt = session.set_counter(&OWNER_SK, 99);

    assert!(
        !receipt.events.is_empty(),
        "set_counter should emit an event"
    );
}

#[test]
fn test_method_returning_reference() {
    let mut session = TestContractSession::new();

    // Initially the label is empty
    assert_eq!(session.label(), "");

    // Update with a new label
    session.update(&OWNER_SK, 10, String::from("hello"));

    // label() returns &String, macro should generate .clone()
    assert_eq!(session.label(), "hello");

    // Verify counter was also updated
    assert_eq!(session.counter(), 10);
}

#[test]
fn test_method_with_reference_parameter() {
    let mut session = TestContractSession::new();

    let item = make_item(1, 100);

    // Not yet added
    assert!(
        !session.contains_item(item),
        "should not contain item before adding"
    );

    // Add and verify lookup by reference
    session.add_item(&OWNER_SK, item);

    assert!(
        session.contains_item(item),
        "should contain item after adding"
    );
    assert!(
        !session.contains_item(make_item(99, 0)),
        "should not contain non-existent item"
    );
}

#[test]
fn test_method_with_multiple_parameters() {
    let mut session = TestContractSession::new();

    // update() takes (u64, String) — macro creates tuple input
    let receipt = session.update(&OWNER_SK, 42, String::from("updated"));

    assert!(
        !receipt.events.is_empty(),
        "update should emit ContractUpdated event"
    );
    assert_eq!(session.counter(), 42);
    assert_eq!(session.label(), "updated");
}

// =============================================================================
// Trait default implementation tests
// =============================================================================

#[test]
fn test_trait_default_implementation_emits_event() {
    let mut session = TestContractSession::new();

    let receipt = session.transfer_ownership(&OWNER_SK, *TEST_PK);

    // Verify the trait's default implementation was called
    assert_eq!(
        session.owner(),
        Some(*TEST_PK),
        "Ownership should have changed — trait default must set new owner"
    );

    assert!(
        !receipt.events.is_empty(),
        "Trait default should emit OwnershipTransferred event"
    );

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
    let mut session = TestContractSession::new();

    assert_eq!(session.owner(), Some(*OWNER_PK));

    // Try to transfer ownership as non-owner
    let result = session.session.call_public::<_, ()>(
        &TEST_SK,
        CONTRACT_ID,
        "transfer_ownership",
        &*OWNER_PK,
    );

    assert!(
        result.is_err(),
        "Non-owner should not be able to transfer ownership"
    );
    assert_eq!(
        session.owner(),
        Some(*OWNER_PK),
        "Ownership should remain unchanged after failed transfer"
    );
}

#[test]
fn test_trait_default_renounce_only_owner() {
    let mut session = TestContractSession::new();

    let result =
        session
            .session
            .call_public::<_, ()>(&TEST_SK, CONTRACT_ID, "renounce_ownership", &());

    assert!(
        result.is_err(),
        "Non-owner should not be able to renounce ownership"
    );
    assert_eq!(
        session.owner(),
        Some(*OWNER_PK),
        "Ownership should remain unchanged after failed renounce"
    );
}

// =============================================================================
// Multiple trait implementation tests
// =============================================================================

#[test]
fn test_multiple_trait_implementations() {
    let mut session = TestContractSession::new();

    // Test Ownable trait methods (first trait impl)
    assert_eq!(
        session.owner(),
        Some(*OWNER_PK),
        "owner() from Ownable should work"
    );

    // Test Versioned trait methods (second trait impl)
    let version_result = session
        .session
        .direct_call::<_, String>(CONTRACT_ID, "version", &())
        .expect("version should succeed");
    assert!(
        !version_result.data.is_empty(),
        "version should return a non-empty string"
    );

    // Verify Ownable still works after using Versioned
    assert_eq!(
        session.owner(),
        Some(*OWNER_PK),
        "owner() should still work"
    );
}

#[test]
fn test_nested_generic_return_type() {
    let mut session = TestContractSession::new();

    // Add an item
    let item = make_item(1, 1000);
    session.add_item(&OWNER_SK, item);

    // find_item returns Option<(ItemId, Item)>
    let id = ItemId(1);
    let result = session
        .session
        .direct_call::<_, Option<(ItemId, Item)>>(CONTRACT_ID, "find_item", &id)
        .expect("find_item should succeed");

    assert!(result.data.is_some(), "Should find the item");
    let (returned_id, returned_item) = result.data.unwrap();
    assert_eq!(returned_id.0, 1, "ItemId should match");
    assert_eq!(returned_item.value, 1000, "Item value should match");

    // Test with non-existent ID
    let missing_id = ItemId(99);
    let result = session
        .session
        .direct_call::<_, Option<(ItemId, Item)>>(CONTRACT_ID, "find_item", &missing_id)
        .expect("find_item should succeed");

    assert!(
        result.data.is_none(),
        "Should return None for non-existent ID"
    );
}

// =============================================================================
// Streaming function tests (abi::feed integration)
// =============================================================================

#[test]
fn test_streaming_function_empty() {
    let mut session = TestContractSession::new();

    let results = session.collect_items();
    assert!(results.is_empty(), "Should return empty when no items");

    let ids = session.collect_item_ids();
    assert!(ids.is_empty(), "Should return empty IDs when no items");
}

#[test]
fn test_streaming_function_single_item() {
    let mut session = TestContractSession::new();

    let item = make_item(1, 1000);
    session.add_item(&OWNER_SK, item);

    let results = session.collect_items();
    assert_eq!(results.len(), 1, "Should have exactly one item");

    let (id, returned_item) = &results[0];
    assert_eq!(id.0, 1, "ItemId should match");
    assert_eq!(returned_item.value, 1000, "Item value should match");

    let ids = session.collect_item_ids();
    assert_eq!(ids.len(), 1, "Should have exactly one ID");
    assert_eq!(ids[0].0, 1, "ID should match");
}

#[test]
fn test_streaming_function_multiple_items() {
    let mut session = TestContractSession::new();

    for i in 1..=5u64 {
        let item = make_item(i, i * 1000);
        session.add_item(&OWNER_SK, item);
    }

    let results = session.collect_items();
    assert_eq!(results.len(), 5, "Should have 5 items");

    let mut found_ids: Vec<u64> = results.iter().map(|(id, _)| id.0).collect();
    found_ids.sort();
    assert_eq!(
        found_ids,
        vec![1, 2, 3, 4, 5],
        "All item IDs should be present"
    );

    for (id, item) in &results {
        let expected_value = id.0 * 1000;
        assert_eq!(
            item.value, expected_value,
            "Value for ID {} should be {}",
            id.0, expected_value
        );
    }

    let ids = session.collect_item_ids();
    assert_eq!(ids.len(), 5, "Should have 5 IDs");

    let mut id_values: Vec<u64> = ids.iter().map(|id| id.0).collect();
    id_values.sort();
    assert_eq!(id_values, vec![1, 2, 3, 4, 5], "All IDs should be present");
}

#[test]
fn test_streaming_function_after_removal() {
    let mut session = TestContractSession::new();

    for i in 1..=3u64 {
        let item = make_item(i, i * 1000);
        session.add_item(&OWNER_SK, item);
    }

    let results = session.collect_items();
    assert_eq!(results.len(), 3, "Should have 3 items initially");

    // Remove item with ID 2
    let id_to_remove = ItemId(2);
    session
        .session
        .call_public::<_, ()>(&OWNER_SK, CONTRACT_ID, "remove_item", &id_to_remove)
        .expect("remove_item should succeed");

    let results = session.collect_items();
    assert_eq!(results.len(), 2, "Should have 2 items after removal");

    let remaining_ids: Vec<u64> = results.iter().map(|(id, _)| id.0).collect();
    assert!(
        !remaining_ids.contains(&2),
        "Removed item should not be in results"
    );
}
