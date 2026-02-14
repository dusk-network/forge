//! Contract deployment and integration tests.

use dusk_core::abi::ContractId;
use dusk_vm::{ContractData, Session, VM};

const CONTRACT_BYTECODE: &[u8] =
    include_bytes!("../target/contract/wasm32-unknown-unknown/release/YOUR_CONTRACT_NAME.wasm");
const CONTRACT_ID: ContractId = ContractId::from_bytes([1; 32]);
const CHAIN_ID: u8 = 1;
const GAS_LIMIT: u64 = u64::MAX;
const OWNER: [u8; 32] = [0; 32];

struct TestHarness {
    _vm: VM,
    session: Session,
}

fn deploy_counter() -> TestHarness {
    let vm = VM::ephemeral().expect("creating ephemeral VM should succeed");
    let mut session = vm.genesis_session(CHAIN_ID);

    let deployed_id = session
        .deploy(
            CONTRACT_BYTECODE,
            ContractData::builder()
                .owner(OWNER)
                .contract_id(CONTRACT_ID),
            GAS_LIMIT,
        )
        .expect("deploying contract should succeed");

    assert_eq!(deployed_id, CONTRACT_ID);

    TestHarness { _vm: vm, session }
}

fn get_count(session: &mut Session) -> u64 {
    session
        .call::<_, u64>(CONTRACT_ID, "get_count", &(), GAS_LIMIT)
        .expect("get_count call should succeed")
        .data
}

#[test]
fn test_contract_deploys_with_zero_state() {
    let mut harness = deploy_counter();
    assert_eq!(get_count(&mut harness.session), 0);
}

#[test]
fn test_counter_mutations() {
    let mut harness = deploy_counter();

    harness
        .session
        .call::<_, ()>(CONTRACT_ID, "increment", &(), GAS_LIMIT)
        .expect("increment call should succeed");
    harness
        .session
        .call::<_, ()>(CONTRACT_ID, "set_count", &42_u64, GAS_LIMIT)
        .expect("set_count call should succeed");
    harness
        .session
        .call::<_, ()>(CONTRACT_ID, "decrement", &(), GAS_LIMIT)
        .expect("decrement call should succeed");

    assert_eq!(get_count(&mut harness.session), 41);
}

#[test]
fn test_decrement_saturates_at_zero() {
    let mut harness = deploy_counter();

    harness
        .session
        .call::<_, ()>(CONTRACT_ID, "decrement", &(), GAS_LIMIT)
        .expect("decrement call should succeed");

    assert_eq!(get_count(&mut harness.session), 0);
}
