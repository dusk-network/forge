# =============================================================================
# Dusk Contract Makefile Template
# =============================================================================
# Copy this entire directory to create a new contract.
#
# Prerequisites:
#   - Rust nightly toolchain with wasm32-unknown-unknown target
#   - jq (for parsing cargo metadata)
#   - wasm-opt (optional, for smaller binaries: https://github.com/WebAssembly/binaryen)
#   - cargo-expand (optional, for macro expansion: cargo install cargo-expand)
#
# Usage:
#   make wasm       - Build optimized contract WASM
#   make wasm-dd    - Build optimized data-driver WASM
#   make all-wasm   - Build both contract and data-driver WASMs
#   make test       - Run all tests
#   make clippy     - Run clippy lints
#   make expand     - Show macro-expanded contract code
#   make clean      - Clean build artifacts
#   make help       - Show all available targets
# =============================================================================

# -----------------------------------------------------------------------------
# Configuration (override these as needed)
# -----------------------------------------------------------------------------

# Contract feature name (adjust to match your Cargo.toml)
CONTRACT_FEATURE ?= contract

# Data-driver feature name (adjust to match your Cargo.toml)
DD_FEATURE ?= data-driver-js

# WASM optimization level: -O0, -O1, -O2, -O3, -O4, -Os, -Oz
WASM_OPT_LEVEL ?= -Oz

# Stack size for contract WASM (64KB default)
STACK_SIZE ?= 65536

# -----------------------------------------------------------------------------
# Auto-detected variables (do not modify)
# -----------------------------------------------------------------------------

CRATE_NAME_DASHED := $(shell cargo metadata --no-deps --format-version 1 \
	--manifest-path Cargo.toml | jq -r '.packages[] | \
	select(.manifest_path == "$(realpath Cargo.toml)") | .name')
CRATE_NAME := $(subst -,_,$(CRATE_NAME_DASHED))

WORKSPACE_ROOT := $(shell cargo metadata --no-deps --format-version 1 \
	--manifest-path Cargo.toml | jq -r '.workspace_root')

WASM_OPT ?= $(shell command -v wasm-opt 2>/dev/null)
WASM_TARGET := wasm32-unknown-unknown

# Contract WASM paths (separate target dir to avoid conflicts)
CONTRACT_TARGET_DIR := $(WORKSPACE_ROOT)/target/contract
CONTRACT_WASM_DIR := $(CONTRACT_TARGET_DIR)/$(WASM_TARGET)/release
CONTRACT_WASM_FILE := $(CONTRACT_WASM_DIR)/$(CRATE_NAME).wasm

# Data-driver WASM paths (separate target dir to avoid conflicts)
DD_TARGET_DIR := $(WORKSPACE_ROOT)/target/data-driver
DD_WASM_DIR := $(DD_TARGET_DIR)/$(WASM_TARGET)/release
DD_WASM_FILE := $(DD_WASM_DIR)/$(CRATE_NAME).wasm

# -----------------------------------------------------------------------------
# Reusable macros
# -----------------------------------------------------------------------------

# Usage: $(call optimize-wasm,path/to/file.wasm)
define optimize-wasm
	@if [ -n "$(WASM_OPT)" ]; then \
		echo "Optimizing with wasm-opt $(WASM_OPT_LEVEL)..."; \
		wasm-opt $(WASM_OPT_LEVEL) --strip-debug $(1) -o $(1); \
	else \
		echo "Note: wasm-opt not found, skipping optimization."; \
		echo "Install binaryen for smaller binaries: https://github.com/WebAssembly/binaryen"; \
	fi
endef

# Usage: $(call require-tool,tool-name,install-command)
define require-tool
	@if ! command -v $(1) >/dev/null 2>&1; then \
		echo "Error: $(1) not found."; \
		echo "Install with: $(2)"; \
		exit 1; \
	fi
endef

# -----------------------------------------------------------------------------
# Default target
# -----------------------------------------------------------------------------

all: wasm

# -----------------------------------------------------------------------------
# Build targets
# -----------------------------------------------------------------------------

.PHONY: wasm
wasm: ## Build contract WASM (optimizes if wasm-opt available)
	@echo "Building contract WASM: $(CRATE_NAME)"
# Contract uses -Z build-std=core,alloc to build a minimal no_std binary
# for on-chain execution. Stack size is constrained for the VM environment.
	@CARGO_TARGET_DIR=$(CONTRACT_TARGET_DIR) \
		RUSTFLAGS="$(RUSTFLAGS) --remap-path-prefix $(HOME)= -C link-args=-zstack-size=$(STACK_SIZE)" \
		cargo build -Z build-std=core,alloc \
			--release \
			--features $(CONTRACT_FEATURE) \
			--color=always \
			--target $(WASM_TARGET)
	$(call optimize-wasm,$(CONTRACT_WASM_FILE))
	@echo "Contract WASM: $(CONTRACT_WASM_FILE)"

.PHONY: wasm-dd
wasm-dd: ## Build data-driver WASM (optimizes if wasm-opt available)
	@echo "Building data-driver WASM: $(CRATE_NAME) (feature: $(DD_FEATURE))"
# Data-driver doesn't use build-std because it runs off-chain (browser/Node)
# and may need std features. No stack size constraint needed.
	@CARGO_TARGET_DIR=$(DD_TARGET_DIR) \
		RUSTFLAGS="$(RUSTFLAGS) --remap-path-prefix $(HOME)=" \
		cargo build \
			--release \
			--features $(DD_FEATURE) \
			--color=always \
			--target $(WASM_TARGET)
	$(call optimize-wasm,$(DD_WASM_FILE))
	@echo "Data-driver WASM: $(DD_WASM_FILE)"

.PHONY: all-wasm
all-wasm: wasm wasm-dd ## Build both contract and data-driver WASMs

# -----------------------------------------------------------------------------
# File targets (for dependency tracking)
# -----------------------------------------------------------------------------

$(CONTRACT_WASM_FILE): wasm
$(DD_WASM_FILE): wasm-dd

# -----------------------------------------------------------------------------
# Test target
# -----------------------------------------------------------------------------

.PHONY: test
test: $(CONTRACT_WASM_FILE) ## Build contract WASM and run tests
	@cargo test --release

# -----------------------------------------------------------------------------
# Development tools
# -----------------------------------------------------------------------------

.PHONY: clippy
clippy: ## Run clippy with strict warnings
	@cargo clippy -Z build-std=core,alloc \
		--release \
		--features $(CONTRACT_FEATURE) \
		--target $(WASM_TARGET) \
		-- -D warnings

.PHONY: expand
expand: ## Show macro-expanded contract code
	$(call require-tool,cargo-expand,cargo install cargo-expand)
	@echo "Expanding macros for: $(CRATE_NAME)"
	@cargo expand --release --features $(CONTRACT_FEATURE) --target $(WASM_TARGET)

.PHONY: expand-dd
expand-dd: ## Show macro-expanded data-driver code
	$(call require-tool,cargo-expand,cargo install cargo-expand)
	@echo "Expanding data-driver macros for: $(CRATE_NAME)"
	@cargo expand --release --features $(DD_FEATURE) --target $(WASM_TARGET)

.PHONY: doc
doc: ## Build documentation
	@cargo doc --release

# -----------------------------------------------------------------------------
# Utility targets
# -----------------------------------------------------------------------------

.PHONY: echo
echo: ## Print contract WASM path
	@echo $(CONTRACT_WASM_FILE)

.PHONY: echo-dd
echo-dd: ## Print data-driver WASM path
	@echo $(DD_WASM_FILE)

.PHONY: clean
clean: ## Clean all build artifacts
	@cargo clean
	@rm -rf $(CONTRACT_TARGET_DIR) $(DD_TARGET_DIR)

.PHONY: help
help: ## Show this help message
	@echo "Dusk Contract Makefile - $(CRATE_NAME)"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'
	@echo ""
	@echo "Configuration (override with make VAR=value):"
	@echo "  CONTRACT_FEATURE  Contract feature name (current: $(CONTRACT_FEATURE))"
	@echo "  DD_FEATURE        Data-driver feature name (current: $(DD_FEATURE))"
	@echo "  WASM_OPT_LEVEL    Optimization level (current: $(WASM_OPT_LEVEL))"
	@echo "  STACK_SIZE        Contract stack size (current: $(STACK_SIZE))"
