CRATE_NAME_DASHED := $(shell cargo metadata --no-deps --format-version 1 --manifest-path Cargo.toml | jq -r '.packages[] | select(.manifest_path == "$(realpath Cargo.toml)") | .name')
CRATE_NAME := $(subst -,_,$(CRATE_NAME_DASHED))

WASM_TARGET = wasm32-unknown-unknown
WASM_RELEASE_DIR = target/$(WASM_TARGET)/release
WASM_FILE = $(WASM_RELEASE_DIR)/$(CRATE_NAME).wasm

# Data-driver WASM output files
DD_WASM_FILE = $(WASM_RELEASE_DIR)/$(CRATE_NAME)_dd.wasm

all: wasm

wasm: ## Generate WASM
	@echo "Building WASM for crate: $(CRATE_NAME)"
	@RUSTFLAGS="$(RUSTFLAGS) --remap-path-prefix $(HOME)= -C link-args=-zstack-size=65536" \
	  cargo build -Z build-std=core,alloc \
			--release \
			--color=always \
			--target $(WASM_TARGET)
	@echo "WASM build complete:"
	@$(MAKE) echo

wasm-dd: ## Generate data-driver WASM
	@echo "Building data-driver WASM for crate: $(CRATE_NAME)"
	@cargo build \
			--release \
			--no-default-features \
			--features data-driver \
			--color=always \
			--target $(WASM_TARGET)
	@mv $(WASM_FILE) $(DD_WASM_FILE)
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

test: wasm ## Run the contract tests
	@cargo test --release --no-default-features \
		-- --test-threads=1

clean:
	@cargo clean

.PHONY: all wasm wasm-dd echo echo-dd clippy expand expand-dd test clean
