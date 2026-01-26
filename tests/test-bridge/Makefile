CRATE_NAME_DASHED := $(shell cargo metadata --no-deps --format-version 1 --manifest-path Cargo.toml | jq -r '.packages[] | select(.manifest_path == "$(realpath Cargo.toml)") | .name')
CRATE_NAME := $(subst -,_,$(CRATE_NAME_DASHED))

# Detect workspace root so we know where target/ actually lives
WORKSPACE_ROOT := $(shell cargo metadata --no-deps --format-version 1 --manifest-path Cargo.toml | jq -r '.workspace_root')

WASM_TARGET = wasm32-unknown-unknown

# Contract WASM target directory
CONTRACT_TARGET_DIR = $(WORKSPACE_ROOT)/target/contract
CONTRACT_WASM_DIR = $(CONTRACT_TARGET_DIR)/$(WASM_TARGET)/release
WASM_FILE = $(CONTRACT_WASM_DIR)/$(CRATE_NAME).wasm

# Data-driver WASM target directory
DD_TARGET_DIR = $(WORKSPACE_ROOT)/target/data-driver
DD_WASM_DIR = $(DD_TARGET_DIR)/$(WASM_TARGET)/release
DD_WASM_FILE = $(DD_WASM_DIR)/$(CRATE_NAME).wasm

all: wasm

wasm: ## Generate contract WASM
	@echo "Building contract WASM for crate: $(CRATE_NAME)"
	@CARGO_TARGET_DIR=$(CONTRACT_TARGET_DIR) \
	  RUSTFLAGS="$(RUSTFLAGS) --remap-path-prefix $(HOME)= -C link-args=-zstack-size=65536" \
	  cargo build -Z build-std=core,alloc \
			--release \
			--color=always \
			--target $(WASM_TARGET)
	@echo "Contract WASM build complete:"
	@$(MAKE) echo

wasm-dd: ## Generate data-driver WASM
	@echo "Building data-driver WASM for crate: $(CRATE_NAME)"
	@CARGO_TARGET_DIR=$(DD_TARGET_DIR) \
	  cargo build \
			--release \
			--no-default-features \
			--features data-driver \
			--color=always \
			--target $(WASM_TARGET)
	@echo "Data-driver WASM build complete:"
	@echo $(DD_WASM_FILE)

echo:
	@echo $(WASM_FILE)

echo-dd:
	@echo $(DD_WASM_FILE)

clippy: ## Run clippy
	@cargo clippy -Z build-std=core,alloc \
		--release \
		--target $(WASM_TARGET) \
	  -- -D warnings

expand: ## Print expanded contract code (default features)
	@if ! command -v cargo-expand >/dev/null 2>&1; then \
		echo "cargo-expand not found. Install with: cargo install cargo-expand"; \
		exit 1; \
	fi
	@echo "Expanding contract macros for crate: $(CRATE_NAME)"
	@cargo expand --release --target $(WASM_TARGET)

expand-dd: ## Print expanded data-driver code
	@if ! command -v cargo-expand >/dev/null 2>&1; then \
		echo "cargo-expand not found. Install with: cargo install cargo-expand"; \
		exit 1; \
	fi
	@echo "Expanding data-driver macros for crate: $(CRATE_NAME)"
	@cargo expand --release --no-default-features --features data-driver --target $(WASM_TARGET)

test: wasm wasm-dd ## Run all tests (contract + schema)
	@cargo test --release --no-default-features \
		-- --test-threads=1

test-contract: wasm ## Run contract tests only
	@cargo test --release --no-default-features --test contract \
		-- --test-threads=1

test-schema: wasm-dd ## Run schema tests only
	@cargo test --release --no-default-features --test schema \
		-- --test-threads=1

clean:
	@cargo clean

.PHONY: all wasm wasm-dd echo echo-dd clippy expand expand-dd test test-contract test-schema clean
