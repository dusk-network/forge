# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add the `dusk-forge` CLI with `new`, `build`, `test`, and `check` commands for contract project scaffolding and workflows.
- Add `expand`, `clean`, and `completions` commands to the `dusk-forge` CLI.
- Add `schema`, `call`, and `verify` commands to the `dusk-forge` CLI.

### Changed

- Move workspace to Rust edition 2024 on the stable toolchain (MSRV 1.85). Generated contract wrappers now use `#[unsafe(no_mangle)]`.
- Remove `-Z build-std=core,alloc` from contract builds (no longer needed on stable).
- Replace EVM-flavored test-bridge with a general-purpose test contract that exercises every `#[contract]` macro code path without domain-specific types.
- Make local forge path overrides opt-in for release builds and harden CLI template/path handling across platforms.

### Fixed

- Make `dusk-forge build data-driver` select the supported project feature (`data-driver-js` or `data-driver`) instead of hardcoding the JS variant.

## [0.2.2] - 2026-02-02

### Changed

- Remove external path dependencies from test-bridge (evm-core, tests-setup)
- Add local `types` crate with bridge types for self-contained testing

### Added

- Add feature-gate compile_error to the contract macro [#3]
- Add compile_error for mutually exclusive contract/data-driver features

### Fixed

- Allow associated functions as contract methods [#6]

### Removed

- Remove `Cargo.lock` from version control
- Remove outdated design documents (`docs/future-type-introspection.md`, `docs/poc.rs`)

## [0.2.1] - 2026-01-29

### Added

- Add changelog

### Fixed

- Fix hero image path

## [0.2.0] - 2026-01-29

### Added

- Add contract test to macro
- Add support for data-driver generation
- Add support for traits

### Changed

- Update repository structure

## [0.1.0] - 2025-01-18

### Added

- Add contract macro for #[no_mangle] scaffolding

<!-- Issues -->
[#3]: https://github.com/dusk-network/forge/issues/3
[#6]: https://github.com/dusk-network/forge/issues/6

<!-- Releases -->
[Unreleased]: https://github.com/dusk-network/forge/compare/v0.2.1...HEAD
[0.2.2]: https://github.com/dusk-network/forge/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/dusk-network/forge/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/dusk-network/forge/releases/tag/v0.2.0
[0.1.0]: https://github.com/Dusk-Forge/dusk-forge/releases/tag/v0.1.0
