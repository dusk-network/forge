# Test Improvement Tasks

This document outlines tasks to improve test coverage and quality for the `dusk-wasm` contract macro workspace. Each task is self-contained and can be tackled independently.

## Overview

Current state (as of analysis):

- **59 unit tests** across macro modules (`parse.rs`, `validate.rs`, `extract.rs`, `generate.rs`, `resolve.rs`)
- **12 integration tests** in `tests/test-bridge/tests/`
- Key gaps: `data_driver.rs` has no unit tests, several macro features lack integration test coverage

---

## Task 1: Add Unit Tests for `data_driver.rs`

**Priority:** High
**Estimated complexity:** Medium
**File:** `contract-macro/src/data_driver.rs`

### Problem

The `data_driver.rs` module generates the `data_driver` module with `ConvertibleContract` implementation. It has zero unit tests - all validation happens indirectly via integration tests that load compiled WASM.

### What to Test

1. **`generate_encode_input_arms`** - Generates match arms for `encode_input_fn`
   - Standard function with simple type
   - Function with tuple input (multiple params)
   - Function with `#[contract(custom)]` attribute (should return error arm)
   - Custom handler function (should call handler)

2. **`generate_decode_input_arms`** - Same cases as encode

3. **`generate_decode_output_arms`** - Generates match arms for `decode_output_fn`
   - Function returning `()`  (should return `JsonValue::Null`)
   - Function returning `u64` (should use `rkyv_to_json_u64`)
   - Function returning complex type
   - Function with `#[contract(custom)]` attribute

4. **`generate_decode_event_arms`** - Generates match arms for `decode_event`
   - Event with constant topic path (e.g., `events::PauseToggled::PAUSED`)
   - Event with string literal topic

5. **`get_resolved_type`** - Type resolution helper
   - Type found in type_map
   - Type not found (fallback to original)

### Implementation Approach

Add a `#[cfg(test)] mod tests` block at the bottom of `data_driver.rs`. Create helper functions similar to other modules:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use quote::{format_ident, quote};
    use std::collections::HashMap;

    fn normalize_tokens(tokens: TokenStream2) -> String {
        tokens.to_string().split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn make_function(name: &str, input: TokenStream2, output: TokenStream2, is_custom: bool) -> FunctionInfo {
        FunctionInfo {
            name: format_ident!("{}", name),
            doc: None,
            params: vec![],
            input_type: input,
            output_type: output,
            is_custom,
            returns_ref: false,
            receiver: crate::Receiver::Ref,
            trait_name: None,
        }
    }

    #[test]
    fn test_encode_input_simple_type() {
        let mut type_map = HashMap::new();
        type_map.insert("Address".to_string(), "evm_core::Address".to_string());

        let functions = vec![make_function("init", quote! { Address }, quote! { () }, false)];
        let arms = generate_encode_input_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"init\""));
        assert!(arm_str.contains("json_to_rkyv"));
        assert!(arm_str.contains("evm_core :: Address"));
    }

    // ... more tests
}
```

### Acceptance Criteria

- [ ] At least 10 unit tests covering the main generation functions
- [ ] Tests verify generated TokenStream contains expected patterns
- [ ] Custom handler integration is tested
- [ ] All tests pass with `cargo test -p contract-macro`

---

## Task 2: Add Reference Return Test to Test-Bridge Contract

**Priority:** High
**Estimated complexity:** Low
**Files:**

- `tests/test-bridge/src/lib.rs`
- `tests/test-bridge/tests/contract.rs`

### Problem

The macro generates `.clone()` calls for methods returning references (`&T` or `&mut T`), but this is never tested in integration tests. See `generate.rs:175-179`:

```rust
if f.returns_ref {
    quote! { STATE.#fn_name(#method_args).clone() }
} else {
    quote! { STATE.#fn_name(#method_args) }
}
```

### Implementation

1. Add a method to `TestBridge` that returns a reference:

```rust
// In tests/test-bridge/src/lib.rs, inside impl TestBridge

/// Returns a reference to the pending withdrawals map.
/// Tests that the macro correctly generates .clone() for reference returns.
pub fn pending_withdrawals_ref(&self) -> &BTreeMap<WithdrawalId, PendingWithdrawal> {
    &self.pending_withdrawals
}
```

2. Add an integration test:

```rust
// In tests/test-bridge/tests/contract.rs

#[test]
fn test_method_returning_reference() {
    let mut session = TestBridgeSession::new();

    // Add a withdrawal first
    let withdrawal = WithdrawalRequest { /* ... */ };
    session.add_pending_withdrawal(&OWNER_SK, withdrawal);

    // Call the method that returns a reference
    // The macro should have generated .clone() so this works
    let withdrawals: BTreeMap<WithdrawalId, PendingWithdrawal> = session
        .session
        .direct_call(TEST_BRIDGE_ID, "pending_withdrawals_ref", &())
        .expect("pending_withdrawals_ref should succeed")
        .data;

    assert!(!withdrawals.is_empty());
}
```

### Acceptance Criteria

- [x] New method `other_bridge_ref` added to test-bridge (returns `&EVMAddress`)
- [x] Integration test calls the method and verifies return value
- [x] Test passes with `make test`

---

## Task 3: Add Reference Parameter Test to Test-Bridge Contract

**Priority:** High
**Estimated complexity:** Low
**Files:**

- `tests/test-bridge/src/lib.rs`
- `tests/test-bridge/tests/contract.rs`

### Problem

The macro handles reference parameters by receiving owned values and passing references. See `generate.rs:299-308`:

```rust
fn generate_arg_expr(param: &ParameterInfo) -> TokenStream2 {
    let name = &param.name;
    if param.is_mut_ref {
        quote! { &mut #name }
    } else if param.is_ref {
        quote! { &#name }
    } else {
        quote! { #name }
    }
}
```

This is tested at unit level but not in integration tests.

### Implementation

1. Add a method with a reference parameter:

```rust
// In tests/test-bridge/src/lib.rs, inside impl TestBridge

/// Verifies data without taking ownership.
/// Tests that the macro correctly handles reference parameters.
pub fn verify_withdrawal(&self, withdrawal: &PendingWithdrawal) -> bool {
    // Simple verification logic
    withdrawal.amount > 0 && withdrawal.block_height > 0
}
```

2. Add integration test:

```rust
// In tests/test-bridge/tests/contract.rs

#[test]
fn test_method_with_reference_parameter() {
    let mut session = TestBridgeSession::new();

    let withdrawal = PendingWithdrawal {
        from: Some(*OWNER_ADDRESS),
        to: EVMAddress([1u8; 20]),
        amount: 1000,
        block_height: 100,
    };

    // The macro should receive PendingWithdrawal and pass &withdrawal to the method
    let is_valid: bool = session
        .session
        .direct_call(TEST_BRIDGE_ID, "verify_withdrawal", &withdrawal)
        .expect("verify_withdrawal should succeed")
        .data;

    assert!(is_valid);
}
```

### Acceptance Criteria

- [x] New method `verify_withdrawal` with `&PendingWithdrawal` parameter added
- [x] Integration test verifies the method works correctly
- [x] Test passes with `make test`

---

## Task 4: Add Multiple Parameters Test

**Priority:** Medium
**Estimated complexity:** Low
**Files:**

- `tests/test-bridge/src/lib.rs`
- `tests/test-bridge/tests/contract.rs`
- `tests/test-bridge/tests/schema.rs`

### Problem

When a method has multiple parameters, the macro creates a tuple input type. This is tested at unit level but not in integration tests.

### Implementation

1. Add a method with multiple parameters:

```rust
// In tests/test-bridge/src/lib.rs, inside impl TestBridge

/// Transfers with explicit fee.
/// Tests tuple parameter handling (from, to, amount) -> (Address, EVMAddress, u64).
pub fn transfer_with_fee(&mut self, from: DSAddress, to: EVMAddress, amount: u64) {
    assert!(!self.is_paused, "bridge is paused");
    // Emit event to verify all params received correctly
    abi::emit(
        events::BridgeInitiated::TOPIC,
        events::BridgeInitiated {
            from: Some(from),
            to,
            amount,
            deposit_fee: 0,
            extra_data: alloc::vec::Vec::new(),
        },
    );
}
```

2. Add integration test:

```rust
#[test]
fn test_method_with_multiple_parameters() {
    let mut session = TestBridgeSession::new();

    let from = *OWNER_ADDRESS;
    let to = EVMAddress([2u8; 20]);
    let amount = 5000u64;

    let receipt = session
        .session
        .call_public(&OWNER_SK, TEST_BRIDGE_ID, "transfer_with_fee", &(from, to, amount))
        .expect("transfer_with_fee should succeed");

    // Verify event was emitted with correct values
    assert!(!receipt.events.is_empty());
}
```

3. Add schema test to verify tuple type in schema:

```rust
#[test]
fn test_schema_function_with_tuple_input() {
    let schema_json = get_schema_from_wasm();
    let schema: serde_json::Value = serde_json::from_str(&schema_json).unwrap();

    let functions = schema["functions"].as_array().unwrap();
    let transfer_fn = functions.iter()
        .find(|f| f["name"] == "transfer_with_fee")
        .expect("transfer_with_fee should be in schema");

    // Input should be a tuple type
    let input = transfer_fn["input"].as_str().unwrap();
    assert!(input.contains("DSAddress") || input.contains("Address"));
    assert!(input.contains("EVMAddress"));
    assert!(input.contains("u64"));
}
```

### Acceptance Criteria

- [x] New method `initiate_transfer` with 3 parameters added (from, to, amount)
- [x] Contract test verifies tuple parameter passing works
- [x] All tests pass with `make test`

---

## Task 5: Add Event Decoding Integration Test

**Priority:** Medium
**Estimated complexity:** Medium
**File:** `tests/test-bridge/tests/schema.rs`

### Problem

The data-driver's `decode_event` method is never tested. The `generate_decode_event_arms` generates match arms, but no test exercises this code path.

### Implementation

Add tests to `schema.rs` that use the `DataDriverWasm` helper to decode events:

```rust
impl DataDriverWasm {
    /// Call decode_event via the WASM interface.
    fn decode_event(&mut self, event_name: &str, rkyv: &[u8]) -> Result<serde_json::Value, String> {
        let memory = self.instance.get_memory(&mut self.store, "memory").unwrap();

        let decode_fn = self.instance
            .get_typed_func::<(i32, i32, i32, i32, i32, i32), i32>(
                &mut self.store,
                "decode_event",
            )
            .expect("Failed to get decode_event function");

        // Similar to decode_output implementation...
        // Write event_name and rkyv to memory, call function, read result
        // ...
    }
}

#[test]
fn test_decode_event_pause_toggled() {
    let mut driver = DataDriverWasm::new();

    // Create rkyv-serialized PauseToggled event data
    // PauseToggled is a unit struct, so serialization is trivial
    let event_data: Vec<u8> = vec![]; // Empty for unit struct

    // The topic constant - need to match what's used in the contract
    let topic = evm_core::standard_bridge::events::PauseToggled::PAUSED;

    let decoded = driver
        .decode_event(topic, &event_data)
        .expect("Failed to decode PauseToggled event");

    // PauseToggled serializes to null or empty object
    assert!(decoded.is_null() || decoded.is_object());
}

#[test]
fn test_decode_event_bridge_initiated() {
    let mut driver = DataDriverWasm::new();

    // Create a BridgeInitiated event and serialize it
    let event = evm_core::standard_bridge::events::BridgeInitiated {
        from: None,
        to: EVMAddress([1u8; 20]),
        amount: 1000,
        deposit_fee: 10,
        extra_data: vec![],
    };

    // Serialize using rkyv (need to add rkyv as dev-dependency if not present)
    let event_data = rkyv::to_bytes::<_, 256>(&event).unwrap().to_vec();

    let topic = evm_core::standard_bridge::events::BridgeInitiated::TOPIC;
    let decoded = driver.decode_event(topic, &event_data).unwrap();

    assert_eq!(decoded["amount"], 1000);
    assert_eq!(decoded["deposit_fee"], 10);
}
```

### Notes

- May need to add `rkyv` as a dev-dependency in test-bridge's `Cargo.toml`
- Need to check the actual topic constant paths used by the contract
- The WASM memory handling follows the existing pattern in `decode_output`

### Acceptance Criteria

- [x] `decode_event` method added to `DataDriverWasm` helper
- [x] At least 2 event decode tests using real contract events (PauseToggled, BridgeInitiated)
- [x] Tests pass with `make test`

---

## Task 6: Add Negative Integration Tests for Data-Driver

**Priority:** Medium
**Estimated complexity:** Low
**File:** `tests/test-bridge/tests/schema.rs`

### Problem

No tests verify error handling in the data-driver. What happens when you call `encode_input_fn` with an unknown function name? What about malformed JSON?

### Implementation

```rust
#[test]
fn test_encode_input_unknown_function() {
    let mut driver = DataDriverWasm::new();

    let result = driver.encode_input("nonexistent_function", "{}");

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("unknown fn"));
}

#[test]
fn test_encode_input_malformed_json() {
    let mut driver = DataDriverWasm::new();

    // "is_paused" expects () input, but we send garbage
    let result = driver.encode_input("is_paused", "not valid json {{{");

    assert!(result.is_err());
}

#[test]
fn test_decode_output_unknown_function() {
    let mut driver = DataDriverWasm::new();

    let result = driver.decode_output("nonexistent_function", &[]);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("unknown fn"));
}

#[test]
fn test_decode_output_malformed_rkyv() {
    let mut driver = DataDriverWasm::new();

    // "is_paused" returns bool, but we send garbage bytes
    let result = driver.decode_output("is_paused", &[0xFF, 0xFF, 0xFF]);

    assert!(result.is_err());
}

#[test]
fn test_decode_event_unknown_topic() {
    let mut driver = DataDriverWasm::new();

    let result = driver.decode_event("unknown::Event::TOPIC", &[]);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("unknown event"));
}
```

### Acceptance Criteria

- [x] At least 5 negative tests added (encode unknown fn, encode malformed JSON, decode output unknown fn, decode output malformed rkyv, decode event unknown topic)
- [x] Tests verify appropriate error codes are returned (also added `get_last_error` helper for error messages)
- [x] Tests pass with `make test`

---

## Task 7: Add Schema Detail Verification Tests

**Priority:** Low
**Estimated complexity:** Low
**File:** `tests/test-bridge/tests/schema.rs`

### Problem

Current schema tests only verify presence of functions/events, not their details. We should verify:

- Input/output types are correct
- Doc comments are captured
- Custom flag is set appropriately

### Implementation

```rust
#[test]
fn test_schema_function_details() {
    let schema_json = get_schema_from_wasm();
    let schema: serde_json::Value = serde_json::from_str(&schema_json).unwrap();

    let functions = schema["functions"].as_array().unwrap();

    // Test is_paused: no input, returns bool
    let is_paused = functions.iter()
        .find(|f| f["name"] == "is_paused")
        .expect("is_paused should exist");
    assert_eq!(is_paused["input"], "()");
    assert_eq!(is_paused["output"], "bool");
    assert_eq!(is_paused["custom"], false);

    // Test init: takes DSAddress, returns ()
    let init = functions.iter()
        .find(|f| f["name"] == "init")
        .expect("init should exist");
    assert!(init["input"].as_str().unwrap().contains("Address"));
    assert_eq!(init["output"], "()");

    // Test deposit: takes Deposit type
    let deposit = functions.iter()
        .find(|f| f["name"] == "deposit")
        .expect("deposit should exist");
    assert!(deposit["input"].as_str().unwrap().contains("Deposit"));
}

#[test]
fn test_schema_doc_comments() {
    let schema_json = get_schema_from_wasm();
    let schema: serde_json::Value = serde_json::from_str(&schema_json).unwrap();

    let functions = schema["functions"].as_array().unwrap();

    // is_paused has doc comment "Returns whether the bridge is paused."
    let is_paused = functions.iter()
        .find(|f| f["name"] == "is_paused")
        .unwrap();
    let doc = is_paused["doc"].as_str().unwrap();
    assert!(doc.contains("paused"), "Doc should mention 'paused', got: {}", doc);
}

#[test]
fn test_schema_event_details() {
    let schema_json = get_schema_from_wasm();
    let schema: serde_json::Value = serde_json::from_str(&schema_json).unwrap();

    let events = schema["events"].as_array().unwrap();

    // Find TransactionDeposited event and verify data type
    let tx_deposited = events.iter()
        .find(|e| e["topic"].as_str().unwrap().contains("TransactionDeposited"))
        .expect("TransactionDeposited event should exist");

    let data_type = tx_deposited["data"].as_str().unwrap();
    assert!(data_type.contains("TransactionDeposited"));
}

#[test]
fn test_schema_import_paths() {
    let schema_json = get_schema_from_wasm();
    let schema: serde_json::Value = serde_json::from_str(&schema_json).unwrap();

    let imports = schema["imports"].as_array().unwrap();

    // Verify Deposit import has full path
    let deposit_import = imports.iter()
        .find(|i| i["name"] == "Deposit")
        .expect("Deposit import should exist");

    let path = deposit_import["path"].as_str().unwrap();
    assert!(path.contains("evm_core"), "Path should be fully qualified: {}", path);
    assert!(path.contains("standard_bridge"));
}
```

### Acceptance Criteria

- [ ] Tests verify function input/output types
- [ ] Tests verify doc comments are captured
- [ ] Tests verify event data types
- [ ] Tests verify import paths are fully qualified
- [ ] All tests pass with `make test`

---

## Task 8: Add Unit Tests for `EmitVisitor` in `lib.rs`

**Priority:** Low
**Estimated complexity:** Low
**File:** `contract-macro/src/lib.rs`

### Problem

The `EmitVisitor` struct finds `abi::emit()` calls in function bodies. It's tested indirectly via integration tests, but has no unit tests.

### Implementation

Add tests at the bottom of `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use syn::visit::Visit;

    #[test]
    fn test_emit_visitor_finds_emit_call() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn pause(&mut self) {
                    self.is_paused = true;
                    abi::emit("paused", PauseEvent { });
                }
            }
        };

        let mut visitor = EmitVisitor::new();
        visitor.visit_item_impl(&impl_block);

        assert_eq!(visitor.events.len(), 1);
        assert_eq!(visitor.events[0].topic, "paused");
    }

    #[test]
    fn test_emit_visitor_finds_const_topic() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn pause(&mut self) {
                    abi::emit(events::PauseToggled::PAUSED, events::PauseToggled());
                }
            }
        };

        let mut visitor = EmitVisitor::new();
        visitor.visit_item_impl(&impl_block);

        assert_eq!(visitor.events.len(), 1);
        assert_eq!(visitor.events[0].topic, "events::PauseToggled::PAUSED");
    }

    #[test]
    fn test_emit_visitor_multiple_emits() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn transfer(&mut self) {
                    abi::emit("started", StartEvent {});
                    // do work
                    abi::emit("completed", CompleteEvent {});
                }
            }
        };

        let mut visitor = EmitVisitor::new();
        visitor.visit_item_impl(&impl_block);

        assert_eq!(visitor.events.len(), 2);
    }

    #[test]
    fn test_emit_visitor_nested_in_if() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn maybe_emit(&mut self, condition: bool) {
                    if condition {
                        abi::emit("conditional", Event {});
                    }
                }
            }
        };

        let mut visitor = EmitVisitor::new();
        visitor.visit_item_impl(&impl_block);

        assert_eq!(visitor.events.len(), 1);
    }

    #[test]
    fn test_extract_doc_comment() {
        let attrs: Vec<Attribute> = vec![
            syn::parse_quote!(#[doc = " First line."]),
            syn::parse_quote!(#[doc = " Second line."]),
        ];

        let doc = extract_doc_comment(&attrs);
        assert!(doc.is_some());
        let doc = doc.unwrap();
        assert!(doc.contains("First line"));
        assert!(doc.contains("Second line"));
    }

    #[test]
    fn test_has_custom_attribute_true() {
        let attrs: Vec<Attribute> = vec![
            syn::parse_quote!(#[contract(custom)]),
        ];
        assert!(has_custom_attribute(&attrs));
    }

    #[test]
    fn test_has_custom_attribute_false() {
        let attrs: Vec<Attribute> = vec![
            syn::parse_quote!(#[doc = "Some doc"]),
        ];
        assert!(!has_custom_attribute(&attrs));
    }
}
```

### Acceptance Criteria

- [ ] At least 7 unit tests for `EmitVisitor` and utility functions
- [ ] Tests cover: simple emit, const topic, multiple emits, nested emits
- [ ] Tests for `extract_doc_comment` and `has_custom_attribute`
- [ ] All tests pass with `cargo test -p contract-macro`

---

## Task 9: Test Multiple Inherent Impl Blocks

**Priority:** Low
**Estimated complexity:** Low
**Files:**

- `tests/test-bridge/src/lib.rs`
- `tests/test-bridge/tests/contract.rs`

### Problem

The macro supports multiple inherent impl blocks for the same struct, but this isn't tested.

### Implementation

Split the `TestBridge` impl block into two:

```rust
// In tests/test-bridge/src/lib.rs

impl TestBridge {
    pub const fn new() -> Self { /* ... */ }
    pub fn init(&mut self, owner: DSAddress) { /* ... */ }
    pub fn is_paused(&self) -> bool { /* ... */ }
    pub fn pause(&mut self) { /* ... */ }
    pub fn unpause(&mut self) { /* ... */ }
}

// Second impl block - tests that macro handles multiple blocks
impl TestBridge {
    pub fn finalization_period(&self) -> u64 { /* ... */ }
    pub fn other_bridge(&self) -> EVMAddress { /* ... */ }
    pub fn deposit(&mut self, deposit: Deposit) { /* ... */ }
    // ... etc
}
```

Ensure existing tests still pass - this validates the macro correctly merges functions from both blocks.

### Acceptance Criteria

- [ ] TestBridge has 2+ inherent impl blocks
- [ ] All existing tests still pass
- [ ] Schema contains functions from all impl blocks

---

## Task 10: Add Trait Default Implementation Test

**Priority:** Low
**Estimated complexity:** Medium
**Files:**

- `tests/test-bridge/src/lib.rs`
- `tests/test-bridge/tests/contract.rs`

### Problem

The macro handles trait methods with empty bodies by calling the trait's default implementation. The `OwnableUpgradeable` trait tests this, but we should verify the behavior more explicitly.

### Current State

`transfer_ownership` and `renounce_ownership` have empty bodies in the test-bridge, signaling "use default impl". The macro generates:

```rust
OwnableUpgradeable::transfer_ownership(&mut STATE, new_owner)
```

### Implementation

Add a test that verifies the trait's default behavior is actually used:

```rust
#[test]
fn test_trait_default_implementation_used() {
    let mut session = TestBridgeSession::new();

    // transfer_ownership's default impl should:
    // 1. Check that caller is owner (only_owner check)
    // 2. Set new owner
    // 3. Emit OwnershipTransferred event

    let receipt = session.transfer_ownership(&OWNER_SK, *TEST_ADDRESS);

    // Verify ownership changed
    assert_eq!(session.owner(), Some(*TEST_ADDRESS));

    // Verify event was emitted (this comes from trait default)
    assert!(!receipt.events.is_empty());

    // The event should be OwnershipTransferred from the trait
    // (If the macro incorrectly used an empty body, no event would be emitted)
}
```

Also verify that non-owner cannot call transfer_ownership (the trait default should panic):

```rust
#[test]
#[should_panic] // or check for error
fn test_trait_default_only_owner_check() {
    let mut session = TestBridgeSession::new();

    // TEST_SK is not the owner, trait default should reject
    let result = session.session.call_public(
        &TEST_SK,
        TEST_BRIDGE_ID,
        "transfer_ownership",
        &*OWNER_ADDRESS
    );

    // Should fail with ownership error
    assert!(result.is_err());
}
```

### Acceptance Criteria

- [ ] Test verifies trait default implementation is called (via event emission)
- [ ] Test verifies trait's access control (only_owner) works
- [ ] Tests pass with `make test`

---

## Checklist Summary

| Task | Priority | Status |
|------|----------|--------|
| 1. Unit tests for `data_driver.rs` | High | [x] |
| 2. Reference return test | High | [x] |
| 3. Reference parameter test | High | [x] |
| 4. Multiple parameters test | Medium | [x] |
| 5. Event decoding test | Medium | [x] |
| 6. Negative integration tests | Medium | [x] |
| 7. Schema detail tests | Low | [ ] |
| 8. `EmitVisitor` unit tests | Low | [ ] |
| 9. Multiple impl blocks test | Low | [ ] |
| 10. Trait default impl test | Low | [ ] |

---

## Running Tests

```bash
# Run all tests
make test

# Run only unit tests (fast)
cargo test -p contract-macro

# Run integration tests (requires WASM build)
cargo test -p test-bridge

# Run specific test
cargo test -p contract-macro test_encode_input_simple_type
```

## Notes for Contributors

- The test-bridge contract must be rebuilt after changes: the Makefile handles this
- Integration tests include WASM via `include_bytes!` - ensure paths match build output
- Schema tests load the data-driver WASM, not the contract WASM
- Use `#[ignore]` for expensive tests that shouldn't run by default
