# Dusk Forge Workspace


Contract macro and tooling for Dusk Network WASM smart contracts.

## Key Files

- `forge/docs/desing.md` - Macro design explanation
- `forge/src/lib.rs` - Main `#[contract]` proc-macro implementation
- `forge/contract-macro/` - Macro internals (parsing, codegen, schema)
- `forge/tests/test-contract/src/lib.rs` - Integration test contract (mirrors general-purpose macro exerciser)
- `forge/contract-template` - Template to start writing your own contract
- `forge/tests/test-contract/tests/` - Contract deployment and schema tests
- `forge/Makefile` - Workspace commands (`make test`, `make clippy`)

## Reference Implementation

The test-contract contract is based on the real general-purpose macro exerciser at:
`/Users/esel/dusk/evm/L1Contracts/general-purpose macro exerciser/src/lib.rs`

Core types come from:
`/Users/esel/dusk/evm/L1Contracts/core/src/`

## Build Commands

Use the Makefiles in all projects to build and test

```bash
make test    # Run all tests (unit + integration)
make clippy  # Run clippy with pedantic warnings
```

## Architecture Notes

- The `#[contract]` macro generates WASM exports and a `CONTRACT_SCHEMA`
- Trait methods with empty bodies signal the macro to use trait defaults
- Schema extraction uses a separate "data-driver" WASM build target

## General Notes

- commit without a `Co-Authored by` line
