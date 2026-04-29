# Contract Macro Design

The `#[contract]` macro enables single-source-of-truth contract development by automatically generating extern wrappers, schemas, and data-driver implementations from annotated contract modules.

## Motivation

Without the macro, contract function signatures are duplicated across multiple files:

```
┌─────────────────────────────────────────────────────────────────────────┐
│ types/src/lib.rs                                                        │
│   - Type definitions (Item, ItemId, events, etc.)                       │
│   - Trait definitions (Ownable)                                         │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ contract/src/state.rs                                                   │
│   - Method signatures: fn add_item(&mut self, item: Item)               │
│   - Business logic                                                      │
│   - Event emissions: abi::emit("topic", event_data)                     │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ contract/src/lib.rs (DUPLICATES state.rs)                               │
│   - N extern "C" fn wrappers                                            │
│   - Repeats every function name                                         │
│   - Repeats input type via wrap_call                                    │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ data-drivers/contract/src/lib.rs (DUPLICATES AGAIN)                     │
│   - N match arms for encode_input_fn                                    │
│   - N match arms for decode_input_fn                                    │
│   - N match arms for decode_output_fn                                   │
│   - M match arms for decode_event                                       │
└─────────────────────────────────────────────────────────────────────────┘
```

Adding a new function requires changes in 3 files, and the duplication leads to drift and bugs.

## Architecture

With the `#[contract]` macro, everything derives from the contract module:

```
┌─────────────────────────────────────────────────────────────────────────┐
│ types/src/lib.rs                                                        │
│   - Type definitions (unchanged, these are shared types)                │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ contract/src/lib.rs  ← SINGLE SOURCE OF TRUTH                           │
│                                                                         │
│   #[contract]                                                           │
│   mod my_contract {                                                     │
│       pub struct MyContract { ... }                                     │
│       impl MyContract {                                                 │
│           pub fn init(&mut self, owner: PublicKey) { ... }              │
│           pub fn counter(&self) -> u64 { ... }                          │
│           pub fn add_item(&mut self, item: Item) { ... }               │
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
use dusk_forge::contract;

#[contract]
mod my_contract {
    use dusk_core::abi;
    use dusk_core::signatures::bls::PublicKey;
    use types::{Item, ItemId, events};

    pub struct MyContract {
        owner: Option<PublicKey>,
        counter: u64,
    }

    impl MyContract {
        /// Creates a new contract instance.
        pub const fn new() -> Self {
            Self {
                owner: None,
                counter: 0,
            }
        }

        /// Initializes the contract with an owner.
        pub fn init(&mut self, owner: PublicKey) {
            self.owner = Some(owner);
        }

        /// Returns the current counter value.
        pub fn counter(&self) -> u64 {
            self.counter
        }

        /// Sets the counter to a new value.
        pub fn set_counter(&mut self, value: u64) {
            let previous = core::mem::replace(&mut self.counter, value);
            abi::emit(
                events::CounterUpdated::TOPIC,
                events::CounterUpdated { previous, new: value },
            );
        }

        /// Adds an item to the collection.
        pub fn add_item(&mut self, item: Item) {
            // ... implementation
            abi::emit(events::Item::ADDED, Item { ..item });
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
| No self | Associated function |
| Parameters after `self` | Input type (tupled if multiple) |
| Return type | Output type (`()` if none) |
| `abi::emit(topic, data)` | Event emission |
| `#[contract(feeds = "Type")]` | Specifies the type fed via `abi::feed()` for streaming functions |
| Doc comments | Included in schema |

### Trait Implementation Exposure

To expose methods from trait implementations, use the `expose` attribute:

```rust
#[contract(expose = [owner, transfer_ownership])]
impl Ownable for MyContract {
    fn owner(&self) -> Option<PublicKey> { self.owner }
    fn owner_mut(&mut self) -> &mut Option<PublicKey> { &mut self.owner }

    // Empty body = use the trait's default implementation
    fn transfer_ownership(&mut self, new_owner: PublicKey) {}
}
```

Only methods listed in `expose` become contract functions. Methods with empty bodies signal the macro to call the trait's default implementation instead.

For traits with associated functions (no `&self`), the same pattern applies:

```rust
pub trait Versioned {
    fn version() -> String {
        String::from(env!("CARGO_PKG_VERSION"))
    }
}

#[contract(expose = [version])]
impl Versioned for MyContract {
    fn version() -> String {}  // empty body = use trait default
}
```

### Streaming Functions (abi::feed)

Some contract functions stream data to the host using `abi::feed()` instead of returning a value directly. These functions return `()` but feed data in chunks that clients need to decode.

Use the `#[contract(feeds = "Type")]` attribute to specify the fed type:

```rust
#[contract]
mod my_contract {
    impl MyContract {
        /// Feeds all items to the host as (ItemId, Item) tuples.
        #[contract(feeds = "(ItemId, Item)")]
        pub fn items(&self) {
            for (id, item) in &self.items {
                abi::feed((*id, *item));
            }
        }

        /// Feeds all item IDs to the host.
        #[contract(feeds = "ItemId")]
        pub fn item_ids(&self) {
            for id in self.items.keys() {
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

## Generated Output

### 1. Contract Schema

A `CONTRACT_SCHEMA` constant is generated at the crate root, containing metadata about all functions and events:

```rust
pub const CONTRACT_SCHEMA: dusk_forge::schema::Contract = dusk_forge::schema::Contract {
    name: "TestContract",
    imports: &[
        Import { name: "Item", path: "types::Item" },
        Import { name: "ItemId", path: "types::ItemId" },
        Import { name: "events", path: "types::events" },
        // ...
    ],
    functions: &[
        Function {
            name: "init",
            doc: "Initializes the contract with an owner.",
            input: "PublicKey",
            output: "()",
        },
        Function {
            name: "counter",
            doc: "Returns the current counter value.",
            input: "()",
            output: "u64",
        },
        // ...
    ],
    events: &[
        Event { topic: "events::CounterUpdated::TOPIC", data: "events::CounterUpdated" },
        Event { topic: "events::CounterReset::TOPIC", data: "events::CounterReset" },
        // ...
    ],
};
```

### 2. Extern "C" Wrappers

When compiled without the `data-driver` feature, extern wrappers are generated for WASM export:

```rust
#[no_mangle]
unsafe extern "C" fn init(arg_len: u32) -> u32 {
    dusk_core::abi::wrap_call(arg_len, |owner: PublicKey| STATE.init(owner))
}

#[no_mangle]
unsafe extern "C" fn counter(arg_len: u32) -> u32 {
    dusk_core::abi::wrap_call(arg_len, |(): ()| STATE.counter())
}

#[no_mangle]
unsafe extern "C" fn add_item(arg_len: u32) -> u32 {
    dusk_core::abi::wrap_call(arg_len, |item: Item| STATE.add_item(item))
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
                "init" => dusk_data_driver::json_to_rkyv::<PublicKey>(json),
                "add_item" => dusk_data_driver::json_to_rkyv::<types::Item>(json),
                "set_counter" => dusk_data_driver::json_to_rkyv::<u64>(json),
                // ...
            }
        }

        fn decode_output_fn(&self, fn_name: &str, rkyv: &[u8]) -> Result<JsonValue, Error> {
            match fn_name {
                "counter" => dusk_data_driver::rkyv_to_json_u64(rkyv),
                "has_items" => dusk_data_driver::rkyv_to_json::<bool>(rkyv),
                "get_item" => dusk_data_driver::rkyv_to_json::<Option<types::Item>>(rkyv),
                // ...
            }
        }

        fn decode_event(&self, event_name: &str, rkyv: &[u8]) -> Result<JsonValue, Error> {
            match event_name {
                types::events::CounterReset::TOPIC =>
                    dusk_data_driver::rkyv_to_json::<types::events::CounterReset>(rkyv),
                types::events::CounterUpdated::TOPIC =>
                    dusk_data_driver::rkyv_to_json::<types::events::CounterUpdated>(rkyv),
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
| `Item` | `types::Item` |
| `ItemId` | `types::ItemId` |
| `events::CounterUpdated` | `types::events::CounterUpdated` |
| `Option<Item>` | `Option<types::Item>` |

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
contract = ["dusk-core/abi-dlmalloc", "types/abi"]
data-driver = ["dep:dusk-data-driver", "dusk-data-driver/wasm-export", "types/serde"]

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
pub fn update(&mut self, counter: u64, label: String)
```

Input type becomes a tuple: `(u64, String)`

### No Parameters (getters)

```rust
pub fn counter(&self) -> u64
```

Input type: `()`, Output type: `u64`

### No Return (mutations)

```rust
pub fn reset_counter(&mut self)
```

Input type: `()`, Output type: `()`

### Reference Returns

```rust
pub fn label(&self) -> &String
```

The wrapper calls `.clone()` before serialization to handle the borrow.

### Reference Parameters

```rust
pub fn contains_item(&self, item: &Item) -> bool
```

The wrapper receives an owned `Item` and passes `&item` to the method.

### Associated Functions

```rust
pub fn empty_id() -> ItemId
```

No `STATE` access needed. The wrapper calls `MyContract::empty_id()` directly.

## Comparison

| Aspect | Without Macro | With Macro |
|--------|--------------|------------|
| Add new function | Edit 3 files | Edit 1 file |
| Lines in lib.rs | ~100 (N externs) | ~0 (generated) |
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
- Type definitions (shared in `types/`)
