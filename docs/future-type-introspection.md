# Future: Type Introspection for Contract Schemas

This document outlines a potential future enhancement to include full type information (struct fields, enum variants) in contract schemas.

## Current Limitation

The `#[contract]` proc macro extracts type **names** but cannot introspect type **definitions**:

```rust
// The macro sees this:
pub fn set_u64(&mut self, value: SetU64) { ... }

// It knows the type is called "SetU64", but cannot see:
pub struct SetU64 {
    pub field: U64Field,
    pub value: u64,
}
```

This is a fundamental Rust limitation: proc macros only have access to the tokens they receive, not to type definitions from other crates or modules.

## Why This Matters

Full type information enables:

- **Client SDK generation**: Automatically generate TypeScript, Python, or other language bindings
- **ABI files**: Similar to Ethereum ABI, enabling cross-language contract interaction
- **Documentation**: Auto-generate API docs with full type details
- **Validation tooling**: Verify data at contract boundaries
- **UI generation**: Build forms or interfaces from schema

## Proposed Solution: Derive Macro

The standard Rust approach is a derive macro that types opt into:

```rust
use dusk_forge::SchemaType;

#[derive(SchemaType)]
pub enum SetU64 {
    FinalizationPeriod(u64),
    DepositFee(u64),
    DepositGasLimit(u64),
    MinGasLimit(u64),
    MaxDataLength(u64),
}

#[derive(SchemaType)]
pub struct Deposit {
    pub to: EVMAddress,
    pub amount: u64,
    pub gas_limit: Option<u64>,
    pub extra_data: Vec<u8>,
}
```

The derive macro generates a trait implementation that describes the type's structure:

```rust
pub trait SchemaType {
    fn schema() -> TypeSchema;
}

pub enum TypeSchema {
    Primitive(&'static str),  // "u64", "bool", etc.
    Struct {
        name: &'static str,
        fields: &'static [FieldSchema],
    },
    Enum {
        name: &'static str,
        variants: &'static [VariantSchema],
    },
    // ... Option, Vec, tuples, etc.
}

pub struct FieldSchema {
    pub name: &'static str,
    pub ty: &'static TypeSchema,
}
```

## How It Would Work

1. **Types derive `SchemaType`**: All types used in contract interfaces must derive the trait

2. **Contract macro references the trait**: Instead of storing just the type name, the macro generates code that calls `T::schema()`:

   ```rust
   // Generated schema would reference:
   FunctionSchema {
       name: "set_u64",
       input: <SetU64 as SchemaType>::schema(),
       output: <() as SchemaType>::schema(),
       // ...
   }
   ```

3. **Primitive types have built-in implementations**: `u8`, `u64`, `bool`, `String`, etc. implement `SchemaType` in the library

4. **Compound types compose**: `Option<T>`, `Vec<T>`, tuples automatically derive schemas from their inner types

## Required Changes

### New Crate or Module

A `dusk-forge-schema` crate (or module) containing:

- `SchemaType` trait definition
- `TypeSchema` enum and related types
- Derive macro for `SchemaType`
- Built-in implementations for primitives and std types

### Contract Macro Updates

- Change `FunctionSchema` to store `TypeSchema` instead of `&'static str`
- Generate trait bounds requiring `SchemaType` for input/output types
- Possibly generate compile errors for types missing the derive

### Ecosystem Impact

- Types in `evm-core`, `dusk-core`, etc. would need to derive `SchemaType`
- This is opt-in but effectively required for contract-facing types
- Ensures only "schema-aware" types cross contract boundaries (a feature, not a bug)

## Alternatives Considered

### Runtime Registry

Types register their schema at program startup:

```rust
inventory::submit! {
    TypeRegistry::register::<SetU64>()
}
```

**Pros**: No trait bounds needed
**Cons**: Runtime overhead, can't guarantee completeness at compile time

### External Schema Files

Define types in a schema language (JSON Schema, protobuf), generate Rust from that:

```json
{
  "SetU64": {
    "fields": {
      "field": "U64Field",
      "value": "u64"
    }
  }
}
```

**Pros**: Language-agnostic source of truth
**Cons**: Inverts the workflow, requires codegen step, types defined twice

### Reflection (Not Available)

Rust has no runtime reflection. The `std::any::type_name` function only provides the type name as a string, not structural information.

## Prior Art

- **ink!/scale-info**: Substrate's smart contract framework uses `scale-info` crate for full type metadata
- **schemars**: Generates JSON Schema from Rust types via derive macro
- **borsh**: Serialization framework with schema generation
- **Ethereum ABI**: JSON format describing function signatures and types

## Conclusion

Adding full type introspection is feasible using the derive macro pattern. It requires:

1. Designing the `TypeSchema` representation
2. Implementing the derive macro
3. Updating existing types to derive `SchemaType`
4. Modifying the contract macro to use trait-based schemas

This is a moderate effort that should be considered when client SDK generation or cross-language interop becomes a priority.
