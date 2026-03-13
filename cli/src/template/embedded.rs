#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKind {
    Counter,
    Empty,
}

#[derive(Debug, Clone, Copy)]
pub struct TemplateFiles {
    pub cargo_toml: &'static str,
    pub lib_rs: &'static str,
    pub test_rs: &'static str,
    pub rust_toolchain_toml: &'static str,
    pub gitignore: &'static str,
    pub makefile: &'static str,
}

const COUNTER_CARGO_TOML: &str = include_str!("../../../contract-template/Cargo.toml");
const COUNTER_LIB_RS: &str = include_str!("../../../contract-template/src/lib.rs");
const COUNTER_TEST_RS: &str = include_str!("../../../contract-template/tests/contract.rs");
const COUNTER_RUST_TOOLCHAIN_TOML: &str = include_str!("../../../rust-toolchain.toml");
const COUNTER_GITIGNORE: &str = include_str!("../../../contract-template/.gitignore");
const COUNTER_MAKEFILE: &str = include_str!("../../../contract-template/Makefile");

const EMPTY_LIB_RS: &str = r#"//! Minimal contract template for `#[contract]`.

#![no_std]
#![cfg(target_family = "wasm")]

#[cfg(not(any(feature = "contract", feature = "data-driver")))]
compile_error!("Enable either 'contract' or 'data-driver' feature for WASM builds");

extern crate alloc;
use dusk_core as _;

#[dusk_forge::contract]
mod YOUR_MODULE_NAME {
    /// Contract state.
    pub struct YOUR_STRUCT_NAME;

    impl YOUR_STRUCT_NAME {
        /// Initialize an empty contract state.
        pub const fn new() -> Self {
            Self
        }
    }
}
"#;

const EMPTY_TEST_RS: &str = r#"//! Contract deployment and integration tests.

use dusk_core::abi::ContractId;
use dusk_vm::{ContractData, VM};

const CONTRACT_BYTECODE: &[u8] =
    include_bytes!("../target/contract/wasm32-unknown-unknown/release/YOUR_CONTRACT_NAME.wasm");

const CONTRACT_ID: ContractId = ContractId::from_bytes([1; 32]);
const CHAIN_ID: u8 = 1;
const GAS_LIMIT: u64 = u64::MAX;
const OWNER: [u8; 32] = [0; 32];

#[test]
fn test_contract_deploys() {
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
}
"#;

pub fn files(template: TemplateKind) -> TemplateFiles {
    match template {
        TemplateKind::Counter => TemplateFiles {
            cargo_toml: COUNTER_CARGO_TOML,
            lib_rs: COUNTER_LIB_RS,
            test_rs: COUNTER_TEST_RS,
            rust_toolchain_toml: COUNTER_RUST_TOOLCHAIN_TOML,
            gitignore: COUNTER_GITIGNORE,
            makefile: COUNTER_MAKEFILE,
        },
        TemplateKind::Empty => TemplateFiles {
            cargo_toml: COUNTER_CARGO_TOML,
            lib_rs: EMPTY_LIB_RS,
            test_rs: EMPTY_TEST_RS,
            rust_toolchain_toml: COUNTER_RUST_TOOLCHAIN_TOML,
            gitignore: COUNTER_GITIGNORE,
            makefile: COUNTER_MAKEFILE,
        },
    }
}
