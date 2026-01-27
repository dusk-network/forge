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

use std::sync::LazyLock;

use dusk_core::abi::ContractId;
use dusk_core::dusk;
use dusk_core::signatures::bls::{PublicKey as AccountPublicKey, SecretKey as AccountSecretKey};
use dusk_vm::CallReceipt;
use evm_core::standard_bridge::{EVMAddress, SetEVMAddressOrOffset};
use evm_core::Address as DSAddress;

use rand::rngs::StdRng;
use rand::SeedableRng;

use tests_setup::TestSession;

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
            .call_public(sender_sk, TEST_BRIDGE_ID, "set_evm_address_or_offset", &value)
            .expect("set_evm_address_or_offset should succeed")
    }

    fn other_bridge_ref(&mut self) -> EVMAddress {
        self.session
            .direct_call::<_, EVMAddress>(TEST_BRIDGE_ID, "other_bridge_ref", &())
            .expect("other_bridge_ref should succeed")
            .data
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
