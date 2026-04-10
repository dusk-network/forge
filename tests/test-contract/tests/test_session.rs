// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Local copy of the `tests-setup` crate's `TestSession` and helpers.
//!
//! This removes the path dependency on `tests-setup`, making the test-contract
//! crate fully standalone.

use dusk_core::abi::{
    CONTRACT_ID_BYTES, ContractError, ContractId, Metadata, StandardBufSerializer,
};
use dusk_core::signatures::bls::{PublicKey as AccountPublicKey, SecretKey as AccountSecretKey};
use dusk_core::stake::STAKE_CONTRACT;
use dusk_core::transfer::data::ContractCall;
use dusk_core::transfer::moonlight::AccountData;
use dusk_core::transfer::phoenix::{Note, PublicKey as ShieldedPublicKey};
use dusk_core::transfer::{TRANSFER_CONTRACT, Transaction};
use dusk_core::{JubJubScalar, LUX};
use dusk_vm::host_queries::{self, HardFork};
use dusk_vm::{CallReceipt, ContractData, Error as VMError, ExecutionConfig, Session, VM, execute};
use ff::Field;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rkyv::bytecheck::CheckBytes;
use rkyv::ser::Serializer;
use rkyv::ser::serializers::{BufferScratch, BufferSerializer, CompositeSerializer};
use rkyv::validation::validators::DefaultValidator;
use rkyv::{Archive, Deserialize, Infallible, Serialize, check_archived_root};

const ZERO_ADDRESS: ContractId = ContractId::from_bytes([0; CONTRACT_ID_BYTES]);
const GAS_LIMIT: u64 = 0x10_000_000;
const CHAIN_ID: u8 = 0x1;
const CONFIG: ExecutionConfig = ExecutionConfig {
    gas_per_deploy_byte: 0u64,
    gas_per_blob: 0u64,
    min_deploy_points: 0u64,
    min_deploy_gas_price: 0u64,
    with_public_sender: true,
    with_blob: true,
    disable_wasm64: false,
    disable_wasm32: false,
    disable_3rd_party: false,
    phoenix_refund_check: false,
};

/// VM Session that has the transfer- and stake-contract deployed and behaves
/// like a mainnet VM.
pub struct TestSession(pub Session);

#[allow(dead_code)]
impl TestSession {
    /// Passes the call to deploy bytecode of a contract to the
    /// underlying session with maximum gas limit.
    pub fn deploy<'a, A, D>(
        &mut self,
        bytecode: &[u8],
        deploy_data: D,
    ) -> Result<ContractId, VMError>
    where
        A: 'a + for<'b> Serialize<StandardBufSerializer<'b>>,
        D: Into<ContractData<'a, A>>,
    {
        self.0.deploy(bytecode, deploy_data, u64::MAX)
    }

    /// Query the transfer-contract for the current chain-id.
    fn chain_id(&self) -> u8 {
        rkyv_deserialize(self.0.meta(Metadata::CHAIN_ID).unwrap())
    }

    /// Query the transfer-contract for the account linked to a given
    /// public-key.
    pub fn account(&mut self, pk: &AccountPublicKey) -> Result<AccountData, VMError> {
        self.0
            .call(TRANSFER_CONTRACT, "account", pk, GAS_LIMIT)
            .map(|r| r.data)
    }

    /// Directly calls the contract, circumventing the transfer contract and
    /// (among other things) also any gas-payment.
    pub fn direct_call<A, R>(
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

    /// Feeder calls are used to have the contract be able to report larger
    /// amounts of data to the host via the channel included in this call.
    pub fn feeder_call<A, R>(
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

    /// Calls the contract through the transfer-contract which is the standard
    /// way any contract is called on the network.
    pub fn call_public<A, R>(
        &mut self,
        sender_sk: &AccountSecretKey,
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
        self.call_public_with_deposit(sender_sk, contract, fn_name, fn_arg, 0)
    }

    /// Calls the contract through the transfer-contract with a deposit.
    pub fn call_public_with_deposit<A, R>(
        &mut self,
        sender_sk: &AccountSecretKey,
        contract: ContractId,
        fn_name: &str,
        fn_arg: &A,
        deposit: u64,
    ) -> Result<CallReceipt<R>, ContractError>
    where
        A: for<'b> Serialize<StandardBufSerializer<'b>>,
        A::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
        R: Archive,
        R::Archived: Deserialize<R, Infallible> + for<'b> CheckBytes<DefaultValidator<'b>>,
    {
        let contract_call = ContractCall {
            contract,
            fn_name: String::from(fn_name),
            fn_args: rkyv_serialize(fn_arg),
        };

        let moonlight_pk = AccountPublicKey::from(sender_sk);

        let AccountData { nonce, .. } = self
            .account(&moonlight_pk)
            .expect("Getting the account should succeed");

        let transaction = Transaction::moonlight(
            &sender_sk,
            None,
            0,
            deposit,
            GAS_LIMIT,
            LUX,
            nonce + 1,
            CHAIN_ID,
            Some(contract_call),
        )
        .expect("Creating moonlight transaction should succeed");

        let _hf = host_queries::set_hard_fork(HardFork::Aegis);
        let receipt = execute(&mut self.0, &transaction, &CONFIG)
            .unwrap_or_else(|e| panic!("Unspendable transaction due to '{e}'"));

        match receipt.data {
            Ok(serialized) => Ok(CallReceipt {
                gas_limit: receipt.gas_limit,
                gas_spent: receipt.gas_spent,
                events: receipt.events,
                call_tree: receipt.call_tree,
                data: rkyv_deserialize(&serialized),
            }),
            Err(e) => Err(e),
        }
    }
}

impl TestSession {
    /// Instantiate the virtual machine with both the transfer and stake
    /// contract deployed.
    pub fn instantiate(
        public_pks: Vec<(&AccountPublicKey, u64)>,
        shielded_pks: Vec<(&ShieldedPublicKey, u64)>,
    ) -> Self {
        let vm = VM::ephemeral().expect("Creating VM should succeed");

        let mut session = VM::genesis_session(&vm, 1);

        // deploy transfer contract
        let transfer_contract = include_bytes!("genesis-contracts/transfer_contract.wasm");

        session
            .deploy(
                transfer_contract,
                ContractData::builder()
                    .owner(ZERO_ADDRESS.to_bytes())
                    .contract_id(TRANSFER_CONTRACT),
                GAS_LIMIT,
            )
            .expect("Deploying the transfer contract should succeed");

        // deploy stake contract
        let stake_contract = include_bytes!("genesis-contracts/stake_contract.wasm");

        session
            .deploy(
                stake_contract,
                ContractData::builder()
                    .owner(ZERO_ADDRESS.to_bytes())
                    .contract_id(STAKE_CONTRACT),
                GAS_LIMIT,
            )
            .expect("Deploying the stake contract should succeed");

        // fund shielded keys with DUSK
        let mut rng = StdRng::seed_from_u64(0xBEEF);
        for (pos, (pk_to_fund, val)) in shielded_pks.iter().enumerate() {
            let value_blinder = JubJubScalar::random(&mut rng);
            let sender_blinder = [
                JubJubScalar::random(&mut rng),
                JubJubScalar::random(&mut rng),
            ];

            let note = Note::obfuscated(
                &mut rng,
                &pk_to_fund,
                &pk_to_fund,
                *val,
                value_blinder,
                sender_blinder,
            );
            session
                .call::<_, Note>(TRANSFER_CONTRACT, "push_note", &(pos, note), GAS_LIMIT)
                .expect("Pushing genesis note should succeed");
        }
        // update the root after the notes have been inserted
        session
            .call(TRANSFER_CONTRACT, "update_root", &(), GAS_LIMIT)
            .map(|r: CallReceipt<()>| r.data)
            .expect("Updating the root should succeed");

        // fund public keys with DUSK
        for (pk_to_fund, val) in &public_pks {
            session
                .call::<_, ()>(
                    TRANSFER_CONTRACT,
                    "add_account_balance",
                    &(**pk_to_fund, *val),
                    GAS_LIMIT,
                )
                .expect("Add account balance should succeed");
        }

        let base = session.commit().expect("Committing should succeed");

        let mut session = TestSession(
            vm.session(base, CHAIN_ID, 1)
                .expect("Instantiating new session should succeed"),
        );

        for (pk, value) in public_pks {
            let account = session
                .account(pk)
                .expect("Getting the account should succeed");
            assert_eq!(
                account.balance, value,
                "The account should own the specified value"
            );
            assert_eq!(account.nonce, 0);
        }

        assert_eq!(
            session.chain_id(),
            CHAIN_ID,
            "the chain id should be as expected"
        );

        session
    }
}

/// Deserialize using `rkyv`.
pub fn rkyv_deserialize<R>(serialized: impl AsRef<[u8]>) -> R
where
    R: Archive,
    R::Archived: Deserialize<R, Infallible> + for<'b> CheckBytes<DefaultValidator<'b>>,
{
    let ta = check_archived_root::<R>(&serialized.as_ref()).expect("Failed to deserialize data");
    ta.deserialize(&mut Infallible)
        .expect("Failed to deserialize using rkyv")
}

/// Serialize using `rkyv`.
pub fn rkyv_serialize<A>(fn_arg: &A) -> Vec<u8>
where
    A: for<'b> Serialize<StandardBufSerializer<'b>>,
    A::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
{
    const SCRATCH_SPACE: usize = 1024;
    const PAGE_SIZE: usize = 0x1000;

    let mut sbuf = [0u8; SCRATCH_SPACE];
    let scratch = BufferScratch::new(&mut sbuf);
    let mut buffer = [0u8; PAGE_SIZE];
    let ser = BufferSerializer::new(&mut buffer[..]);
    let mut ser = CompositeSerializer::new(ser, scratch, Infallible);

    ser.serialize_value(fn_arg)
        .expect("Failed to rkyv serialize fn_arg");
    let pos = ser.pos();

    buffer[..pos].to_vec()
}

#[allow(dead_code)]
pub fn assert_contract_panic<R>(
    call_result: Result<CallReceipt<R>, ContractError>,
    expected_panic: &str,
) where
    R: Archive,
    R::Archived: Deserialize<R, Infallible> + for<'b> CheckBytes<DefaultValidator<'b>>,
{
    let contract_err = match call_result {
        Ok(_) => panic!("Contract call shouldn't pass"),
        Err(error) => error,
    };

    if let ContractError::Panic(panic_msg) = contract_err {
        assert_eq!(panic_msg, expected_panic);
    } else {
        panic!("Expected contract panic, got error: {contract_err}",);
    }
}
