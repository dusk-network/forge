# Contract Macro Design

The `#[contract]` macro enables single-source-of-truth contract development by automatically generating extern wrappers, schemas, and data-driver implementations from annotated contract modules.

## Motivation

Without the macro, contract function signatures are duplicated across multiple files:

```
┌─────────────────────────────────────────────────────────────────────────┐
│ core/standard_bridge.rs                                                 │
│   - Type definitions (Deposit, SetU64, WithdrawalId, etc.)              │
│   - Event types                                                         │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ StandardBridge/src/state.rs                                             │
│   - Method signatures: fn deposit(&mut self, d: Deposit)                │
│   - Business logic                                                      │
│   - Event emissions: abi::emit("topic", event_data)                     │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ StandardBridge/src/lib.rs (DUPLICATES state.rs)                         │
│   - 34 extern "C" fn wrappers                                           │
│   - Repeats every function name                                         │
│   - Repeats input type via wrap_call                                    │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ data-drivers/StandardBridge/src/lib.rs (DUPLICATES AGAIN)               │
│   - 50+ match arms for encode_input_fn                                  │
│   - 50+ match arms for decode_input_fn                                  │
│   - 50+ match arms for decode_output_fn                                 │
│   - 15+ match arms for decode_event                                     │
└─────────────────────────────────────────────────────────────────────────┘
```

Adding a new function requires changes in 3 files, and the duplication leads to drift and bugs.

## Architecture

With the `#[contract]` macro, everything derives from the contract module:

```
┌─────────────────────────────────────────────────────────────────────────┐
│ core/standard_bridge.rs                                                 │
│   - Type definitions (unchanged, these are shared types)                │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ StandardBridge/src/lib.rs  ← SINGLE SOURCE OF TRUTH                     │
│                                                                         │
│   #[contract]                                                           │
│   mod standard_bridge {                                                 │
│       pub struct StandardBridge { ... }                                 │
│       impl StandardBridge {                                             │
│           pub fn init(&mut self, owner: DSAddress) { ... }              │
│           pub fn is_paused(&self) -> bool { ... }                       │
│           pub fn deposit(&mut self, d: Deposit) { ... }                 │
│       }                                                                 │
│   }                                                                     │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
              ┌──────────┐   ┌──────────┐   ┌──────────────┐
              │ extern C │   │  Schema  │   │ Data-Driver  │
              │ wrappers │   │  (JSON)  │   │    module    │
              │ (gen'd)  │   │  (gen'd) │   │   (gen'd)    │
              └──────────┘   └──────────┘   └──────────────┘
```

## Usage

### Contract Module Structure

The macro expects a module containing:
- Import statements for types used in function signatures
- A single public struct (the contract state)
- An impl block with a `const fn new() -> Self` constructor
- Public methods that become contract functions

```rust
use dusk_wasm::contract;

#[contract]
mod my_contract {
    use evm_core::standard_bridge::{Deposit, SetU64, WithdrawalId};
    use evm_core::standard_bridge::events;
    use evm_core::Address as DSAddress;

    pub struct MyContract {
        owner: Option<DSAddress>,
        is_paused: bool,
    }

    impl MyContract {
        /// Creates a new contract instance.
        pub const fn new() -> Self {
            Self {
                owner: None,
                is_paused: false,
            }
        }

        /// Initializes the contract with an owner.
        pub fn init(&mut self, owner: DSAddress) {
            self.owner = Some(owner);
        }

        /// Returns whether the contract is paused.
        pub fn is_paused(&self) -> bool {
            self.is_paused
        }

        /// Pauses the contract.
        pub fn pause(&mut self) {
            self.is_paused = true;
            abi::emit(events::PauseToggled::PAUSED, events::PauseToggled());
        }

        /// Deposits funds.
        pub fn deposit(&mut self, deposit: Deposit) {
            // ... implementation
            abi::emit(events::TransactionDeposited::TOPIC, events::TransactionDeposited { ... });
        }

        /// Custom serialization needed for this function.
        #[contract(custom)]
        pub fn extra_data(&self, pk: PublicKey) -> Vec<u8> {
            encode_ds_address(pk)
        }

        // Private helper - NOT exposed as contract function
        fn only_owner(&self) {
            // ...
        }
    }
}
```

### Detection Rules

The macro analyzes the contract module and extracts metadata:

| Source | Interpretation |
|--------|---------------|
| `pub fn` | Exported contract function |
| `fn` (private) | Internal helper, not exported |
| `&self` | Read-only method |
| `&mut self` | Mutating method |
| Parameters after `self` | Input type (tupled if multiple) |
| Return type | Output type (`()` if none) |
| `abi::emit(topic, data)` | Event emission |
| `#[contract(custom)]` | Requires manual encode/decode in data-driver |
| Doc comments | Included in schema |

### Trait Implementation Exposure

To expose methods from trait implementations, use the `expose` attribute:

```rust
#[contract(expose = [call_a, call_b])]
impl SomeTrait for MyContract {
    fn call_a(&mut self) { ... }
    fn call_b(&self) -> u64 { ... }
    fn internal_helper(&self) { ... }  // Not exposed
}
```

## Generated Output

### 1. Contract Schema

A `CONTRACT_SCHEMA` constant is generated at the crate root, containing metadata about all functions and events:

```rust
pub const CONTRACT_SCHEMA: dusk_wasm::schema::Contract = dusk_wasm::schema::Contract {
    name: "MyContract",
    imports: &[
        Import { name: "Deposit", path: "evm_core::standard_bridge::Deposit" },
        Import { name: "DSAddress", path: "evm_core::Address" },
        // ...
    ],
    functions: &[
        Function {
            name: "init",
            doc: "Initializes the contract with an owner.",
            input: "DSAddress",
            output: "()",
            custom: false,
        },
        Function {
            name: "is_paused",
            doc: "Returns whether the contract is paused.",
            input: "()",
            output: "bool",
            custom: false,
        },
        // ...
    ],
    events: &[
        Event { topic: "events::PauseToggled::PAUSED", data: "events::PauseToggled" },
        Event { topic: "events::TransactionDeposited::TOPIC", data: "events::TransactionDeposited" },
        // ...
    ],
};
```

### 2. Extern "C" Wrappers

When compiled without the `data-driver` feature, extern wrappers are generated for WASM export:

```rust
#[no_mangle]
unsafe extern "C" fn init(arg_len: u32) -> u32 {
    dusk_core::abi::wrap_call(arg_len, |owner: DSAddress| STATE.init(owner))
}

#[no_mangle]
unsafe extern "C" fn is_paused(arg_len: u32) -> u32 {
    dusk_core::abi::wrap_call(arg_len, |(): ()| STATE.is_paused())
}

#[no_mangle]
unsafe extern "C" fn deposit(arg_len: u32) -> u32 {
    dusk_core::abi::wrap_call(arg_len, |d: Deposit| STATE.deposit(d))
}
```

### 3. Data-Driver Module

When compiled with the `data-driver` feature, a `data_driver` module is generated instead:

```rust
#[cfg(feature = "data-driver")]
pub mod data_driver {
    pub struct Driver;

    impl dusk_data_driver::ConvertibleContract for Driver {
        fn encode_input_fn(&self, fn_name: &str, json: &str) -> Result<Vec<u8>, Error> {
            match fn_name {
                "init" => dusk_data_driver::json_to_rkyv::<evm_core::Address>(json),
                "deposit" => dusk_data_driver::json_to_rkyv::<evm_core::standard_bridge::Deposit>(json),
                // ...
            }
        }

        fn decode_output_fn(&self, fn_name: &str, rkyv: &[u8]) -> Result<JsonValue, Error> {
            match fn_name {
                "is_paused" => dusk_data_driver::rkyv_to_json::<bool>(rkyv),
                // ...
            }
        }

        fn decode_event(&self, event_name: &str, rkyv: &[u8]) -> Result<JsonValue, Error> {
            match event_name {
                evm_core::standard_bridge::events::PauseToggled::PAUSED =>
                    dusk_data_driver::rkyv_to_json::<evm_core::standard_bridge::events::PauseToggled>(rkyv),
                // ...
            }
        }

        fn get_schema(&self) -> String {
            super::CONTRACT_SCHEMA.to_json()
        }
    }

    dusk_data_driver::generate_wasm_entrypoint!(Driver);
}
```

## Macro Internals

### Module Structure

The macro is implemented in `contract-macro/` with the following modules:

| Module | Purpose |
|--------|---------|
| `lib.rs` | Entry point, orchestration, `EmitVisitor` for finding events |
| `parse.rs` | Parses `use` statements to extract import paths |
| `validate.rs` | Validates method signatures (no async, no generics, etc.) |
| `extract.rs` | Extracts functions, events, and metadata from the AST |
| `generate.rs` | Generates schema, state variable, and extern wrappers |
| `resolve.rs` | Resolves short type names to fully-qualified paths |
| `data_driver.rs` | Generates the data-driver module |

### Type Resolution

The macro resolves short type names to fully-qualified paths using the import statements:

| Short Name | Resolved Path |
|------------|--------------|
| `Deposit` | `evm_core::standard_bridge::Deposit` |
| `DSAddress` (aliased) | `evm_core::Address` |
| `events::PauseToggled` | `evm_core::standard_bridge::events::PauseToggled` |
| `Option<Deposit>` | `Option<evm_core::standard_bridge::Deposit>` |

This ensures the data-driver can reference types correctly even though it's in a different module context.

### Feature Gating

The contract module and data-driver module are mutually exclusive via feature flags:

```rust
// Generated output structure:

pub const CONTRACT_SCHEMA: Contract = /* ... */;

#[cfg(not(feature = "data-driver"))]
mod my_contract {
    // Contract struct, impl, STATE, extern wrappers
}

#[cfg(feature = "data-driver")]
pub mod data_driver {
    // Driver struct implementing ConvertibleContract
}
```

## Cargo Configuration

Contracts using the macro need feature flags in `Cargo.toml`:

```toml
[features]
default = ["contract"]
contract = ["dusk-core/abi-dlmalloc", "evm-core/abi"]
data-driver = ["dep:dusk-data-driver", "dusk-data-driver/wasm-export", "evm-core/serde"]

[dependencies]
dusk-core = { workspace = true }
dusk-data-driver = { workspace = true, optional = true }
```

Build commands:
- `cargo build --target wasm32-unknown-unknown` → Contract WASM
- `cargo build --target wasm32-unknown-unknown --no-default-features --features data-driver` → Data-driver WASM

## Edge Cases

### Multiple Parameters

```rust
pub fn verify_sig(&self, threshold: u8, msg: Vec<u8>, sig: Signature) -> bool
```

Input type becomes a tuple: `(u8, Vec<u8>, Signature)`

### No Parameters (getters)

```rust
pub fn is_paused(&self) -> bool
```

Input type: `()`, Output type: `bool`

### No Return (mutations)

```rust
pub fn pause(&mut self)
```

Input type: `()`, Output type: `()`

### Reference Returns

```rust
pub fn get_data(&self) -> &SomeType
```

The wrapper calls `.clone()` before serialization to handle the borrow.

### Custom Serialization

```rust
#[contract(custom)]
pub fn extra_data(&self, pk: PublicKey) -> Vec<u8>
```

Marked in schema with `custom: true`. The data-driver returns an "unsupported" error for these functions - custom handlers must be implemented manually if needed.

## Comparison

| Aspect | Without Macro | With Macro |
|--------|--------------|------------|
| Add new function | Edit 3 files | Edit 1 file |
| Lines in lib.rs | ~100 (34 externs) | ~0 (generated) |
| Separate data-driver crate | Required (~300 lines) | Not needed (generated) |
| Schema | Manual / `todo!()` | Auto-generated |
| Schema drift | Possible | Impossible |
| Type safety | Runtime errors | Compile-time |
| Doc comments in schema | No | Yes |

## Summary

The `#[contract]` macro provides:

- **Single source of truth**: Contract logic in one place
- **Auto-generated**: Extern wrappers, schema, data-driver
- **Type-safe**: Compile-time verification of signatures
- **Documented**: Doc comments flow into schema
- **Minimal annotation**: Just `#[contract]` on the module

Manual work remaining:
- Type definitions (shared in `core/`)
- Custom serialization handlers (rare, via `#[contract(custom)]`)
