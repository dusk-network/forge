# dusk-wasm

Contract macro and tooling for Dusk Network WASM smart contracts.

The `#[contract]` macro eliminates boilerplate by automatically generating WASM exports, schemas, and data-driver implementations from a single annotated contract module.

## Quick Start

### 1. Create a New Contract

Copy the `contract-template` directory and rename it:

```bash
cp -r contract-template my-contract
cd my-contract
```

Update `Cargo.toml`:

- Replace `YOUR_CONTRACT_NAME` with your contract name (e.g., `my-contract`)
- Add any additional dependencies your contract needs

### 2. Write Your Contract

Edit `src/lib.rs`:

```rust
#![no_std]
#![cfg(target_family = "wasm")]

extern crate alloc;

#[dusk_wasm::contract]
mod my_contract {
    use dusk_core::abi;

    /// Contract state.
    pub struct MyContract {
        value: u64,
    }

    impl MyContract {
        /// Creates a new contract instance.
        pub const fn new() -> Self {
            Self { value: 0 }
        }

        /// Returns the current value.
        pub fn get_value(&self) -> u64 {
            self.value
        }

        /// Sets the value.
        pub fn set_value(&mut self, value: u64) {
            self.value = value;
        }
    }
}
```

### 3. Build the Contract

The template includes a Makefile that handles building, optimization, and testing:

```bash
make wasm      # Build optimized contract WASM
make test      # Build and run tests
make expand    # Show macro-expanded code (useful for debugging)
make help      # Show all available targets
```

The contract WASM will be at `target/contract/wasm32-unknown-unknown/release/my_contract.wasm`

> **Note:** The template enables `overflow-checks = true` in release builds. This is critical for contract security - never disable it.

## Contract Structure

The `#[contract]` macro expects:

| Element | Requirement |
|---------|-------------|
| Module | Annotated with `#[dusk_wasm::contract]` |
| Struct | Single public struct (the contract state) |
| Constructor | `pub const fn new() -> Self` |
| Methods | `pub fn` methods become contract functions |

### Method Visibility

```rust
impl MyContract {
    // Public methods are exported as contract function
    pub fn deposit(&mut self, amount: u64) { ... }

    // Private methods are internal helper, they will NOT be exported
    fn validate(&self) -> bool { ... }
}
```

### Parameter Handling

| Signature | Input Type | Output Type |
|-----------|------------|-------------|
| `fn get(&self) -> u64` | `()` | `u64` |
| `fn set(&mut self, v: u64)` | `u64` | `()` |
| `fn transfer(&mut self, to: Address, amount: u64)` | `(Address, u64)` | `()` |

Multiple parameters are automatically tupled.

## Events

Emit events using `abi::emit`:

```rust
use dusk_core::abi;

pub fn transfer(&mut self, to: Address, amount: u64) {
    // ... transfer logic ...

    abi::emit("transfer", TransferEvent { from: self.owner, to, amount });
}
```

Events are automatically detected and included in the contract schema.

## Trait Implementations

Expose trait methods using the `expose` attribute:

```rust
#[contract(expose = [owner, transfer_ownership, renounce_ownership])]
impl Ownable for MyContract {
    fn owner(&self) -> Address {
        self.owner
    }

    fn owner_mut(&mut self) -> &mut Address {
        &mut self.owner  // NOT exposed (not in list)
    }

    // Empty body = use trait's default implementation
    fn transfer_ownership(&mut self, new: Address) {}

    // Empty body = use trait's default implementation
    fn renounce_ownership(&mut self) {}
}
```

- Only methods listed in `expose` become contract functions
- **Empty method bodies** signal the macro to use the trait's default implementation
- Methods with actual implementations use your code

## Streaming Functions

For functions that stream data via `abi::feed()`:

```rust
/// Streams all pending items to the host.
#[contract(feeds = "(ItemId, Item)")]
pub fn get_all_items(&self) {
    for (id, item) in &self.items {
        abi::feed((*id, item.clone()));
    }
}
```

The `feeds` attribute tells the data-driver what type to decode.

## Data-Driver

The data-driver is a separate WASM build that provides JSON encoding/decoding for external tools (wallets, explorers, etc.).

### Building the Data-Driver

```bash
make wasm-dd    # Build data-driver WASM
make expand-dd  # Show macro-expanded data-driver code (useful for debugging)
```

The data-driver WASM will be at `target/data-driver/wasm32-unknown-unknown/release/my_contract.wasm`

### Data-Driver WASM Exports

The data-driver WASM exports these functions:

| Export | Description |
|--------|-------------|
| `init` | Initialize the driver (call once at startup) |
| `get_schema` | Returns the contract schema as JSON |
| `encode_input_fn` | Encodes JSON input for a contract function call |
| `decode_output_fn` | Decodes rkyv output to JSON |
| `decode_event` | Decodes rkyv event data to JSON |

For JavaScript integration, use [w3sper](https://github.com/dusk-network/rusk/tree/master/w3sper.js) which provides a high-level API for working with data-drivers.

### Custom Serialization

For types requiring custom encoding/decoding:

```rust
#[contract]
mod my_contract {
    // ... contract impl ...

    /// Custom encoder for the "special_data" function.
    #[contract(encode_input = "special_data")]
    fn encode_special(json: &str) -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error> {
        // Custom encoding logic
    }

    /// Custom decoder for the "special_data" function.
    #[contract(decode_output = "special_data")]
    fn decode_special(rkyv: &[u8]) -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error> {
        // Custom decoding logic
    }
}
```

## Contract Schema

The macro generates a `CONTRACT_SCHEMA` constant with metadata:

```rust
// Access the schema
let schema_json = CONTRACT_SCHEMA.to_json();
```

The schema includes:

- Contract name
- All public functions with their input/output types
- Doc comments
- Events with topics and data types
- Import paths for type resolution

## Cargo.toml Configuration

Contracts have **two build targets** from the same source:

1. **Contract WASM** - Runs on-chain in the Dusk VM
2. **Data-driver WASM** - Runs off-chain for JSON encoding/decoding

### Dependencies

All runtime dependencies go in the WASM-only section because contracts are gated by `#![cfg(target_family = "wasm")]`:

```toml
[target.'cfg(target_family = "wasm")'.dependencies]
dusk-core = "1.4"
dusk-data-driver = { version = "0.3", optional = true }  # Only for data-driver
dusk-wasm = "0.1"

[dev-dependencies]
dusk-core = "1.4"   # Same types, but for host-side tests
dusk-vm = "0.1"     # To run contract in tests
```

### Features

```toml
[features]
# Contract WASM - uses custom allocator for on-chain execution
contract = ["dusk-core/abi-dlmalloc"]

# Data-driver WASM - enable serde for JSON serialization
data-driver = [
  "dusk-core/serde",
  "dep:dusk-data-driver",
  "dusk-data-driver/wasm-export",
]

# Data-driver with memory exports for JavaScript
data-driver-js = ["data-driver", "dusk-data-driver/alloc"]
```

The `contract` and `data-driver` features are **mutually exclusive** - never enable both at the same time. The Makefile handles this by explicitly selecting one feature per build target.

### Adding Dependencies

| Dependency Type | Where to Add | Feature Flags |
|----------------|--------------|---------------|
| Both builds | WASM-only section | None needed |
| Contract-only | WASM-only section with `optional = true` | Add `dep:name` to `contract` feature |
| Data-driver-only | WASM-only section with `optional = true` | Add `dep:name` to `data-driver` feature |

If a dependency has types used in function signatures, also add `name/serde` to the `data-driver` feature to enable JSON serialization.

### Overflow Checks

Always enable overflow checks for contract safety:

```toml
[profile.release]
overflow-checks = true
```

This prevents integer overflow vulnerabilities. The contract template includes this by default - never remove it.

## Makefile Targets

The contract template includes a Makefile with the following targets:

| Target | Description |
|--------|-------------|
| `make wasm` | Build optimized contract WASM |
| `make wasm-dd` | Build optimized data-driver WASM |
| `make all-wasm` | Build both contract and data-driver |
| `make test` | Build WASMs and run tests |
| `make clippy` | Run clippy with strict warnings |
| `make expand` | Show macro-expanded contract code |
| `make expand-dd` | Show macro-expanded data-driver code |
| `make clean` | Clean all build artifacts |
| `make help` | Show all targets and configuration |

### Configuration

Override Makefile variables as needed:

```bash
make wasm CONTRACT_FEATURE=contract  # Custom contract feature name
make wasm WASM_OPT_LEVEL=-Os         # Use -Os instead of -Oz
make wasm STACK_SIZE=131072          # 128KB stack instead of 64KB
make wasm-dd DD_FEATURE=data-driver  # Use data-driver instead of data-driver-js
```

### Prerequisites

- Rust nightly toolchain with `wasm32-unknown-unknown` target
- `jq` (for parsing cargo metadata)
- `wasm-opt` (optional, for smaller binaries - install via [binaryen](https://github.com/WebAssembly/binaryen))
- `cargo-expand` (optional, for `make expand` - install via `cargo install cargo-expand`)

## Project Structure

```
dusk-wasm/
├── src/lib.rs          # Re-exports the contract macro
├── contract-macro/     # Proc-macro implementation
├── contract-template/  # Template for new contracts
├── tests/test-bridge/  # Integration tests
└── docs/
    └── design.md       # Detailed macro internals
```

## Development

```bash
# Run all tests
make test

# Run clippy
make clippy

# Show available commands
make help
```

## License

This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
