# Workspace Makefile for dusk-forge

.PHONY: all test test-unit test-integration clippy cq fmt check doc clean help

# rustfmt.toml uses unstable options, so formatting needs nightly. It's pinned because
# formatting changes between nightlies. Override with `make fmt NIGHTLY=...`.
NIGHTLY ?= nightly-2026-08-19

all: test

test: test-unit test-integration ## Run all tests

test-unit: ## Run unit tests
	@echo "Running unit tests..."
	@cargo test -p dusk-forge-contract
	@# Without RUSTC_WRAPPER, like clippy below, until dusk-network/.github#63 is fixed.
	@RUSTC_WRAPPER= cargo test -p dusk-forge-cli
	@cargo test --release

test-integration: ## Run integration tests (test-contract)
	@$(MAKE) -C tests/test-contract test

fmt: ## Format code (requires the pinned nightly)
	@rustup run $(NIGHTLY) rustfmt --version >/dev/null 2>&1 || rustup toolchain install $(NIGHTLY) --profile minimal --component rustfmt
	@cargo +$(NIGHTLY) fmt --all $(if $(CHECK),-- --check,)

check: ## Run cargo check on all targets
	@cargo check --all-targets

doc: ## Generate documentation
	@cargo doc --no-deps

cq: ## Run code quality checks (formatting + clippy)
	@$(MAKE) fmt CHECK=1
	@$(MAKE) clippy

clippy: ## Run clippy on all workspace members
	@echo "Running clippy..."
	@# Clear RUSTC_WRAPPER: in CI, builds through sccache lose CARGO_BIN_EXE_dusk-forge,
	@# which the CLI's integration tests read at compile time. Remove this once
	@# dusk-network/.github#63 is fixed.
	@RUSTC_WRAPPER= cargo clippy --workspace --exclude test-contract --all-targets -- -D warnings
	@$(MAKE) -C tests/test-contract clippy

clean: ## Clean all build artifacts
	@cargo clean
	@$(MAKE) -C tests/test-contract clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'
