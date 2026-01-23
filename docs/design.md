# Data-Driver Automation Design

## Single Source of Truth: `state.rs`

The contract implementation in `state.rs` is the **only** place where function signatures exist. Everything else should be derived from it.

### Current Reality (Multiple Sources of Truth)

```
┌─────────────────────────────────────────────────────────────────────────┐
│ SOURCE 1: core/standard_bridge.rs                                       │
│   - Type definitions (Deposit, SetU64, WithdrawalId, etc.)              │
│   - Event types                                                         │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ SOURCE 2: StandardBridge/src/state.rs                                   │
│   - Method signatures: fn deposit(&mut self, d: Deposit)                │
│   - Business logic                                                      │
│   - Event emissions: abi::emit("topic", event_data)                     │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ SOURCE 3: StandardBridge/src/lib.rs (DUPLICATES SOURCE 2)               │
│   - 34 extern "C" fn wrappers                                           │
│   - Repeats every function name                                         │
│   - Repeats input type via wrap_call                                    │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ SOURCE 4: data-drivers/StandardBridge/src/lib.rs (DUPLICATES 2 & 3)     │
│   - 50+ match arms for encode_input_fn                                  │
│   - 50+ match arms for decode_input_fn                                  │
│   - 50+ match arms for decode_output_fn                                 │
│   - 15+ match arms for decode_event                                     │
│   - Schema: todo!()                                                     │
└─────────────────────────────────────────────────────────────────────────┘
```

**Problem:** Function names and types are repeated 4 times. Adding a new function requires changes in 3 files.

---

## Goal: Derive Everything from `state.rs`

```
┌─────────────────────────────────────────────────────────────────────────┐
│ core/standard_bridge.rs                                                 │
│   - Type definitions (unchanged, these are shared types)                │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ StandardBridge/src/state.rs  ← SINGLE SOURCE OF TRUTH                   │
│                                                                         │
│   #[contract]                                                           │
│   impl StandardBridge {                                                 │
│       pub fn init(&mut self, owner: DSAddress) { ... }                  │
│       pub fn is_paused(&self) -> bool { ... }                           │
│       pub fn deposit(&mut self, d: Deposit) { ... }                     │
│   }                                                                     │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
              ┌──────────┐   ┌──────────┐   ┌──────────────┐
              │ extern C │   │  Schema  │   │ Data-Driver  │
              │ wrappers │   │  (JSON)  │   │    impl      │
              │ (gen'd)  │   │  (gen'd) │   │   (gen'd)    │
              └──────────┘   └──────────┘   └──────────────┘
```

---

## Design: Minimal Annotation

### What the developer writes:

```rust
// In StandardBridge/src/state.rs

use evm_core::contract;  // The only import needed

#[contract]
impl StandardBridge {
    /// Initializes the contract with an owner.
    pub fn init(&mut self, owner: DSAddress) {
        assert!(!self.initialized);
        self.owner = Some(owner);
        self.initialized = true;
    }

    /// Returns whether the bridge is paused.
    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    /// Pauses the bridge. Only callable by owner.
    pub fn pause(&mut self) {
        self.only_owner();
        self.is_paused = true;
        abi::emit("paused", events::PauseToggled());
    }

    /// Returns the finalization period in blocks.
    pub fn finalization_period(&self) -> u64 {
        self.finalization_period
    }

    /// Updates a u64 configuration value.
    pub fn set_u64(&mut self, new_value: SetU64) {
        self.only_owner();
        // ... implementation
        abi::emit("u64_set", events::U64Set { ... });
    }

    /// Deposits funds to L2.
    pub fn deposit(&mut self, deposit: Deposit) {
        // ... implementation
        abi::emit("transaction_deposited", events::TransactionDeposited { ... });
        abi::emit("bridge_initiated", events::BridgeInitiated { ... });
    }

    /// Returns a pending withdrawal by ID.
    pub fn pending_withdrawal(&self, id: WithdrawalId) -> Option<PendingWithdrawal> {
        self.pending_withdrawals.get(&id).cloned()
    }

    /// Finalizes a withdrawal after the finalization period.
    pub fn finalize_withdrawal(&mut self, id: WithdrawalId) {
        // ... implementation
        abi::emit("bridge_finalized", events::BridgeFinalized { ... });
    }

    /// Custom serialization needed for this function.
    #[contract(custom)]
    pub fn extra_data(&self, pk: PublicKey) -> Vec<u8> {
        encode_ds_address(pk)
    }

    /// Internal helper - NOT exposed as contract function.
    fn only_owner(&self) {
        // Private functions are not exported
    }
}
```

### What the macro extracts:

| Method | Input Type | Output Type | Events Emitted |
|--------|-----------|-------------|----------------|
| `init` | `DSAddress` | `()` | - |
| `is_paused` | `()` | `bool` | - |
| `pause` | `()` | `()` | `paused` → `PauseToggled` |
| `finalization_period` | `()` | `u64` | - |
| `set_u64` | `SetU64` | `()` | `u64_set` → `U64Set` |
| `deposit` | `Deposit` | `()` | `transaction_deposited`, `bridge_initiated` |
| `pending_withdrawal` | `WithdrawalId` | `Option<PendingWithdrawal>` | - |
| `finalize_withdrawal` | `WithdrawalId` | `()` | `bridge_finalized` |
| `extra_data` | *custom* | *custom* | - |

**Detection rules:**
- `pub fn` → exported contract function
- `fn` (private) → internal, not exported
- `&self` with return → getter (input = `()`)
- `&mut self` → mutating operation
- Parameters after `self` → input type (tupled if multiple)
- Return type → output type (`()` if none)
- `abi::emit("topic", data)` → event (topic + data type)
- `#[contract(custom)]` → needs manual encode/decode

---

## What Gets Generated

### 1. Extern "C" Wrappers (in same crate)

```rust
// Auto-generated, replaces manual lib.rs

#[no_mangle]
unsafe extern "C" fn init(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |owner: DSAddress| STATE.init(owner))
}

#[no_mangle]
unsafe extern "C" fn is_paused(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |(): ()| STATE.is_paused())
}

#[no_mangle]
unsafe extern "C" fn deposit(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |d: Deposit| STATE.deposit(d))
}

// ... all 34 functions generated
```

### 2. Contract Schema (exported constant)

```rust
// Auto-generated in contract crate

pub const CONTRACT_SCHEMA: Schema = Schema {
    name: "StandardBridge",
    functions: &[
        Function {
            name: "init",
            doc: "Initializes the contract with an owner.",
            input: TypeInfo::of::<DSAddress>(),
            output: TypeInfo::of::<()>(),
            custom: false,
        },
        Function {
            name: "is_paused",
            doc: "Returns whether the bridge is paused.",
            input: TypeInfo::of::<()>(),
            output: TypeInfo::of::<bool>(),
            custom: false,
        },
        // ... all functions
    ],
    events: &[
        Event { topic: "paused", data: TypeInfo::of::<events::PauseToggled>() },
        Event { topic: "transaction_deposited", data: TypeInfo::of::<events::TransactionDeposited>() },
        Event { topic: "bridge_initiated", data: TypeInfo::of::<events::BridgeInitiated>() },
        Event { topic: "bridge_finalized", data: TypeInfo::of::<events::BridgeFinalized>() },
        Event { topic: "u64_set", data: TypeInfo::of::<events::U64Set>() },
        // ... detected from abi::emit calls
    ],
};
```

### 3. Data-Driver Implementation

```rust
// In data-drivers/StandardBridge/src/lib.rs
// ENTIRE FILE:

use standard_bridge::CONTRACT_SCHEMA;

generate_data_driver!(CONTRACT_SCHEMA);

// Only custom handlers need implementation
#[custom_handler(extra_data)]
fn encode_extra_data(json: &str) -> Result<Vec<u8>, Error> {
    let pk: PublicKey = serde_json::from_str(json)?;
    Ok(encode_ds_address(pk))
}

#[custom_handler(extra_data)]
fn decode_extra_data(rkyv: &[u8]) -> Result<JsonValue, Error> {
    let pk = decode_ds_address(rkyv)?;
    Ok(serde_json::to_value(pk)?)
}
```

That's it. **~15 lines** instead of **~300 lines**.

### 4. JSON Schema (for clients)

```json
{
  "contract": "StandardBridge",
  "functions": [
    {
      "name": "init",
      "description": "Initializes the contract with an owner.",
      "input": { "$ref": "#/types/DSAddress" },
      "output": null
    },
    {
      "name": "deposit",
      "description": "Deposits funds to L2.",
      "input": {
        "type": "object",
        "properties": {
          "to": { "$ref": "#/types/EVMAddress" },
          "amount": { "type": "integer", "format": "uint64" },
          "fee": { "type": "integer", "format": "uint64" },
          "extra_data": { "type": "string", "format": "hex" }
        }
      },
      "output": null
    }
  ],
  "events": [
    { "topic": "paused", "data": { "$ref": "#/types/PauseToggled" } },
    { "topic": "transaction_deposited", "data": { "$ref": "#/types/TransactionDeposited" } }
  ],
  "types": {
    "DSAddress": { "oneOf": [{ "$ref": "#/types/External" }, { "$ref": "#/types/Contract" }] },
    "EVMAddress": { "type": "string", "pattern": "^0x[a-fA-F0-9]{40}$" }
  }
}
```

---

## Implementation Plan

### Phase 1: `#[contract]` Proc Macro ✅ IMPLEMENTED

**Crate:** `contract-macro` (in `dusk-wasm`)

**Input:** Annotated impl block
**Output:**
- Original impl block (unchanged)
- `CONTRACT_SCHEMA` constant
- Extern "C" wrapper functions

```rust
#[proc_macro_attribute]
pub fn contract(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let impl_block: ItemImpl = parse(item);

    let functions = extract_public_methods(&impl_block);
    let events = extract_emit_calls(&impl_block);

    let schema = generate_schema(&functions, &events);
    let externs = generate_extern_wrappers(&functions);

    quote! {
        #impl_block

        #schema

        #externs
    }
}
```

### Phase 2: Data-Driver Module Generation ✅ IMPLEMENTED

**Approach:** Generate a `data_driver` module at crate root level, feature-gated with
`#[cfg(feature = "data-driver")]`. The contract module is wrapped with
`#[cfg(not(feature = "data-driver"))]`, making them mutually exclusive.

**Key insight:** The `#[contract]` macro already has all the type information at expansion
time. Rather than trying to read `CONTRACT_SCHEMA` at runtime (which proc macros cannot do),
we generate the data-driver implementation alongside the contract code.

**Implementation:**

1. **Type Resolution (`resolve.rs`):** Resolves short type names to fully-qualified paths
   at extraction time, handling:
   - Simple types: `Deposit` → `evm_core::standard_bridge::Deposit`
   - Aliases: `DSAddress` → `evm_core::Address`
   - Multi-segment paths: `events::PauseToggled` → `evm_core::standard_bridge::events::PauseToggled`
   - Generics: `Option<Deposit>` → `Option<evm_core::standard_bridge::Deposit>`
   - Event topic paths: `events::PauseToggled::PAUSED` → full path

2. **Data-Driver Generation (`data_driver.rs`):** Generates the `Driver` struct and
   `ConvertibleContract` implementation using pre-resolved type paths.

3. **Feature-Gated Output:** The macro outputs:
   ```rust
   #[cfg(not(feature = "data-driver"))]
   mod contract_name {
       // Contract code, schema, extern wrappers
   }

   #[cfg(feature = "data-driver")]
   pub mod data_driver {
       // Driver struct implementing ConvertibleContract
       // WASM entrypoint via generate_wasm_entrypoint!
   }
   ```

**Contract Cargo.toml configuration:**
```toml
[features]
default = ["contract"]
contract = ["dusk-core/abi-dlmalloc", "evm-core/abi"]
data-driver = ["dep:dusk-data-driver", "dusk-data-driver/wasm-export", "evm-core/serde"]

[dependencies]
dusk-core = { workspace = true }  # Always available for types
dusk-data-driver = { workspace = true, optional = true }
```

**Build commands:**
- `make wasm` → Contract WASM (default features)
- `make wasm-dd` → Data-driver WASM (`--no-default-features --features data-driver`)

### Phase 3: Event Detection

Parse `abi::emit()` calls to extract event topics and types:

```rust
fn extract_emit_calls(impl_block: &ItemImpl) -> Vec<Event> {
    let mut events = Vec::new();

    for method in &impl_block.items {
        visit_expr(method, |expr| {
            if let Expr::Call(call) = expr {
                if is_abi_emit(call) {
                    let topic = extract_string_literal(&call.args[0]);
                    let event_type = extract_type(&call.args[1]);
                    events.push(Event { topic, event_type });
                }
            }
        });
    }

    events
}
```

---

## Comparison

| Aspect | Before | After (Phase 1 + 2) |
|--------|--------|---------------------|
| Add new function | Edit 3 files | Edit 1 file |
| Lines in lib.rs | ~100 (34 externs) | 0 (generated) |
| Lines in data-driver crate | ~300 | 0 (generated in contract crate) |
| Separate data-driver crate | Required | Not needed |
| Schema | `todo!()` | Auto-generated |
| Can schema drift? | Yes (manual sync) | No (derived) |
| Type safety | Runtime errors | Compile-time |
| Doc comments in schema | No | Yes |

---

## Edge Cases

### Multiple Parameters

```rust
pub fn verify_sig(&self, threshold: u8, msg: Vec<u8>, sig: Signature) -> bool
```

**Becomes:** Input type = `(u8, Vec<u8>, Signature)` (auto-tupled)

### No Parameters (getters)

```rust
pub fn is_paused(&self) -> bool
```

**Becomes:** Input type = `()`, Output type = `bool`

### No Return (mutations)

```rust
pub fn pause(&mut self)
```

**Becomes:** Input type = `()`, Output type = `()`

### Custom Serialization

```rust
#[contract(custom)]
pub fn extra_data(&self, pk: PublicKey) -> Vec<u8>
```

**Becomes:** Marked in schema, requires manual handler in data-driver

### Private Methods

```rust
fn only_owner(&self)  // No `pub` = not exported
```

**Becomes:** Ignored by macro

---

## Files Changed

### Contract Crate

```diff
  // StandardBridge/src/state.rs
+ #[contract]
  impl StandardBridge {
      // Methods unchanged, just add #[contract(custom)] where needed
  }
```

```diff
  // StandardBridge/src/lib.rs
- #[no_mangle]
- unsafe extern "C" fn init(arg_len: u32) -> u32 { ... }
- #[no_mangle]
- unsafe extern "C" fn is_paused(arg_len: u32) -> u32 { ... }
- // ... 32 more functions
+ // Empty or minimal - externs are generated
```

### Data-Driver (No Separate Crate Needed)

The data-driver is now generated directly in the contract crate, eliminating the need
for a separate `data-drivers/StandardBridge` crate:

```diff
  // StandardBridge/Cargo.toml
+ [features]
+ default = ["contract"]
+ contract = ["dusk-core/abi-dlmalloc", "evm-core/abi"]
+ data-driver = ["dep:dusk-data-driver", "dusk-data-driver/wasm-export", "evm-core/serde"]
```

**Build both WASMs from the same crate:**
- `make wasm` → `standard_bridge.wasm` (177K)
- `make wasm-dd` → `standard_bridge_dd.wasm` (342K)

The separate `data-drivers/StandardBridge` crate (~300 lines) can be removed entirely.

---

## Summary

**Single source of truth:** `state.rs` method signatures

**What's derived:**
- Extern "C" wrappers
- Contract schema
- Data-driver implementation
- JSON schema for clients
- Function documentation

**What's manual:**
- Type definitions (shared in `core/`)
- Custom serialization handlers (rare)

**Result:** Add a function once in `state.rs`, everything else updates automatically.
