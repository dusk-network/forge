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
| `#[contract(feeds = "Type")]` | Specifies the type fed via `abi::feed()` for streaming functions |
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

### Streaming Functions (abi::feed)

Some contract functions stream data to the host using `abi::feed()` instead of returning a value directly. These functions return `()` but feed data in chunks that clients need to decode.

Use the `#[contract(feeds = "Type")]` attribute to specify the fed type:

```rust
#[contract]
mod my_contract {
    impl MyContract {
        /// Feeds all pending withdrawals to the host.
        #[contract(feeds = "(WithdrawalId, PendingWithdrawal)")]
        pub fn pending_withdrawals(&self) {
            for (id, pending) in &self.pending_withdrawals {
                abi::feed((*id, *pending));
            }
        }

        /// Feeds all finalized withdrawal IDs to the host.
        #[contract(feeds = "WithdrawalId")]
        pub fn finalized_withdrawals(&self) {
            for id in &self.finalized_withdrawals {
                abi::feed(*id);
            }
        }
    }
}
```

The macro uses the `feeds` type for `decode_output_fn` in the data-driver instead of the return type, allowing clients to correctly decode the streamed data.

#### Compile-Time Validation

The macro validates `feeds` usage and produces helpful error messages:

| Error | Cause |
|-------|-------|
| Missing `#[contract(feeds = "Type")]` | Function uses `abi::feed()` but lacks the attribute |
| Multiple `abi::feed()` calls | Only one feed call site is allowed per function |
| Tuple mismatch | Attribute specifies tuple type but expression doesn't look like a tuple (or vice versa) |

These checks catch common mistakes at compile time rather than runtime.

### Custom Data-Driver Functions

Sometimes you need data-driver functions that don't correspond to actual contract methods—for example, utility functions for encoding/decoding addresses or other custom serialization. These are "virtual" functions that only exist in the data-driver.

Use the `#[contract(encode_input = "fn_name")]`, `#[contract(decode_input = "fn_name")]`, or `#[contract(decode_output = "fn_name")]` attributes on functions to define custom handlers:

```rust
#[contract]
mod my_contract {
    // ... contract impl ...

    /// Custom encoder for the "extra_data" data-driver function.
    /// This function is NOT a contract method - it only exists in the data-driver.
    #[contract(encode_input = "extra_data")]
    fn encode_extra_data(json: &str) -> Result<Vec<u8>, dusk_data_driver::Error> {
        let pk: dusk_core::signatures::bls::PublicKey = serde_json::from_str(json)?;
        Ok(evm_core::standard_bridge::encode_ds_address(pk))
    }

    /// Custom decoder for the "extra_data" data-driver function.
    #[contract(decode_output = "extra_data")]
    fn decode_extra_data(rkyv: &[u8]) -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error> {
        let pk = evm_core::standard_bridge::decode_ds_address(rkyv)?;
        Ok(serde_json::to_value(pk)?)
    }
}
```

**Important:** These handler functions are moved into the generated `data_driver` module during macro expansion, so they must use fully-qualified paths for all types (except those available in the data-driver module like `dusk_data_driver::Error` and `alloc::vec::Vec`).

The macro will:
1. Remove these functions from the contract module (they're not contract methods)
2. Move them into the generated `data_driver` module
3. Generate match arms that call them for the specified function name

Each data-driver function can have up to three handlers:
- `encode_input`: Called by `encode_input_fn(fn_name, json) -> Vec<u8>`
- `decode_input`: Called by `decode_input_fn(fn_name, rkyv) -> JsonValue`
- `decode_output`: Called by `decode_output_fn(fn_name, rkyv) -> JsonValue`

If a handler is not provided for a role, the data-driver will return an "Unsupported" error for that operation.

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
contract = ["dusk-core/abi-dlmalloc", "evm-core/abi"]
data-driver = ["dep:dusk-data-driver", "dusk-data-driver/wasm-export", "evm-core/serde"]

[dependencies]
dusk-core = { workspace = true }
dusk-data-driver = { workspace = true, optional = true }
```

No default feature is defined. Build commands explicitly select the feature:
- `cargo build --target wasm32-unknown-unknown --features contract` → Contract WASM
- `cargo build --target wasm32-unknown-unknown --features data-driver` → Data-driver WASM

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
