# Workspace Makefile for dusk-wasm

.PHONY: all test test-unit test-integration clippy clean help

all: test

test: test-unit test-integration ## Run all tests

test-unit: ## Run unit tests
	@echo "Running unit tests..."
	@cargo test --release

test-integration: ## Run integration tests (test-bridge)
	@$(MAKE) -C tests/test-bridge test

clippy: ## Run clippy on all workspace members
	@echo "Running clippy..."
	@cargo clippy --all-targets -- -D warnings
	@$(MAKE) -C tests/test-bridge clippy

clean: ## Clean all build artifacts
	@cargo clean
	@$(MAKE) -C tests/test-bridge clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'
