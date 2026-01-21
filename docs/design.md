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

### Phase 1: `#[contract]` Proc Macro

**Crate:** `dusk-contract-macros`

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

### Phase 2: `generate_data_driver!` Macro

**Crate:** `dusk-data-driver`

**Input:** Schema constant
**Output:** Complete `ConvertibleContract` implementation

```rust
#[proc_macro]
pub fn generate_data_driver(input: TokenStream) -> TokenStream {
    let schema_path: Path = parse(input);

    // Generate match arms for each function
    let encode_arms = generate_encode_arms(&schema);
    let decode_input_arms = generate_decode_input_arms(&schema);
    let decode_output_arms = generate_decode_output_arms(&schema);
    let decode_event_arms = generate_decode_event_arms(&schema);

    quote! {
        pub struct ContractDriver;

        impl ConvertibleContract for ContractDriver {
            fn encode_input_fn(&self, fn_name: &str, json: &str) -> Result<Vec<u8>, Error> {
                match fn_name {
                    #encode_arms
                    name => Err(Error::Unsupported(format!("fn {name}")))
                }
            }
            // ... other methods
        }
    }
}
```

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

| Aspect | Current | With Macros |
|--------|---------|-------------|
| Add new function | Edit 3 files | Edit 1 file (state.rs) |
| Lines in lib.rs | ~100 (34 externs) | 0 (generated) |
| Lines in data-driver | ~300 | ~15 (custom only) |
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

### Data-Driver Crate

```diff
  // data-drivers/StandardBridge/src/lib.rs
- impl ConvertibleContract for ContractDriver {
-     fn encode_input_fn(&self, fn_name: &str, json: &str) -> Result<Vec<u8>, Error> {
-         match fn_name {
-             "init" => json_to_rkyv::<DSAddress>(json),
-             "is_paused" => json_to_rkyv::<()>(json),
-             // ... 50+ more arms
-         }
-     }
-     // ... 3 more methods with 50+ arms each
- }
+ use standard_bridge::CONTRACT_SCHEMA;
+ generate_data_driver!(CONTRACT_SCHEMA);
+
+ #[custom_handler(extra_data)]
+ fn encode_extra_data(json: &str) -> Result<Vec<u8>, Error> { ... }
+
+ #[custom_handler(extra_data)]
+ fn decode_extra_data(rkyv: &[u8]) -> Result<JsonValue, Error> { ... }
```

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
