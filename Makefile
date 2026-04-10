# Workspace Makefile for dusk-forge

.PHONY: all test test-unit test-integration clippy cq fmt check doc clean help

all: test

test: test-unit test-integration ## Run all tests

test-unit: ## Run unit tests
	@echo "Running unit tests..."
	@cargo test -p dusk-forge-contract
	@cargo test --release

test-integration: ## Run integration tests (test-contract)
	@$(MAKE) -C tests/test-contract test

fmt: ## Format code (requires nightly)
	@rustup component add --toolchain nightly rustfmt 2>/dev/null || true
	@cargo +nightly fmt --all $(if $(CHECK),-- --check,)

check: ## Run cargo check on all targets
	@cargo check --all-targets

doc: ## Generate documentation
	@cargo doc --no-deps

cq: ## Run code quality checks (formatting + clippy)
	@$(MAKE) fmt CHECK=1
	@$(MAKE) clippy

clippy: ## Run clippy on all workspace members
	@echo "Running clippy..."
	@cargo clippy --all-targets -- -D warnings
	@$(MAKE) -C tests/test-contract clippy

clean: ## Clean all build artifacts
	@cargo clean
	@$(MAKE) -C tests/test-contract clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'
