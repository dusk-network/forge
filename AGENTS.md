# Forge

Smart contract development framework for the Dusk network. Provides a `#[contract]` proc-macro that generates WASM exports and contract schemas, a CLI for scaffolding and building contract projects, and integration tests exercising every macro code path.

## Workspace Layout

```
forge/
├── src/                    # dusk-forge — re-exports and schema types
├── contract-macro/         # dusk-forge-contract — proc-macro (#[contract])
├── cli/                    # dusk-forge-cli — CLI binary (new, build, test, schema, call, verify)
├── tests/types/            # types — helper types for integration tests
├── tests/test-contract/    # test-contract — general-purpose macro exerciser
├── contract-template/      # Template for scaffolding new contract projects
├── docs/                   # Design documents
├── Makefile                # Workspace-level targets
└── rust-toolchain.toml     # Stable toolchain with wasm32 target
```

| Directory | Crate | Kind |
|-----------|-------|------|
| `/` (root) | `dusk-forge` | Library |
| `contract-macro/` | `dusk-forge-contract` | Proc-macro |
| `cli/` | `dusk-forge-cli` | Binary |
| `tests/types/` | `types` | Library (test helper) |
| `tests/test-contract/` | `test-contract` | Contract (integration test) |

## Commands

Run `make help` to see all available targets.

## Architecture

### `#[contract]` Macro

The `#[contract]` attribute macro (in `contract-macro/`) transforms a Rust module into a Dusk WASM smart contract:

- **Parsing** (`extract.rs`, `resolve.rs`) — extracts method signatures, trait impls, and state types from the annotated module
- **Validation** (`validate.rs`) — enforces contract rules (e.g., mutually exclusive feature gates for `contract` vs `data-driver`)
- **Code generation** (`generate.rs`) — emits `#[no_mangle]` WASM export wrappers with rkyv (de)serialization
- **Schema generation** (`data_driver.rs`) — produces a `CONTRACT_SCHEMA` constant describing the contract's ABI for data-driver builds

Trait methods with empty bodies signal the macro to use trait defaults.

### CLI

The `dusk-forge` CLI (`cli/`) provides project management commands:

- `new` — scaffold a contract project from the template
- `build` — compile contract and data-driver WASM binaries
- `test` / `check` / `clean` / `expand` — development workflow wrappers
- `schema` — extract and display the contract schema from a data-driver WASM
- `call` / `verify` — invoke contract methods and verify results via wasmtime

### Test Contract

`tests/test-contract/` is a general-purpose macro exerciser that covers every `#[contract]` code path: owned methods, borrowed methods, trait implementations, associated functions, and schema generation.

## Conventions

- **`no_std`** for contract crates. The proc-macro and CLI are `std`.
- **`--release` for tests**: integration tests in `tests/test-contract/` build WASM binaries, which require release mode.
- **Edition 2024** with MSRV 1.85 (stable toolchain).
- **Serialization**: `rkyv` for contract state and arguments, `serde` for schema JSON.
- **Feature gates**: Contracts use mutually exclusive features (`contract` vs `data-driver` / `data-driver-js`) to produce different WASM binaries from the same source.

## Change Propagation

| Changed | Also verify |
|---------|-------------|
| `dusk-forge` / `contract-macro` | `tests/test-contract`, `duskevm-contracts`, downstream contract repos |

## Git Conventions

- Default branch: `main`
- License: MPL-2.0

### Commit messages

Format: `<scope>: <Description>` — imperative mood, capitalize first word after colon.

**One commit per scope per concern.** Each commit touches one logical scope and one concern. Don't bundle unrelated changes.

Canonical scopes:

| Scope | Directory |
|-------|-----------|
| `forge` | Root crate (`src/`) |
| `macro` | `contract-macro/` |
| `cli` | `cli/` |
| `test-contract` | `tests/test-contract/` |
| `types` | `tests/types/` |
| `workspace` | Root `Cargo.toml`, Makefile, `rust-toolchain.toml` |
| `ci` | `.github/workflows/` |
| `chore` | Formatting, config files, tooling |

Examples:
- `macro: Add support for generic trait impls`
- `cli: Fix data-driver feature detection`
- `test-contract: Add borrowed-state method test`
- `workspace: Update dusk-core dependency`

### Changelog

Maintain `CHANGELOG.md` with entries under `[Unreleased]` using [keep-a-changelog](https://keepachangelog.com/) format. If a change traces to a GitHub issue, reference it as a link: `[#42](https://github.com/dusk-network/forge/issues/42)`. Only link to GitHub issues.
