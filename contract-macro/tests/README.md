# `dusk-forge-contract` test harness

End-to-end coverage of the `#[contract]` macro. Five layers:

| Layer | What it pins | Driven by |
|---|---|---|
| Unit tests under `src/` | Pure-Rust validation / parsing helpers | `cargo test -p dusk-forge-contract` |
| `tests/compile-fail/` (+ `compile_fail.rs`) | Each rejection rule produces a diagnostic | trybuild |
| `tests/compile-pass/` (+ `compile_pass.rs`) | Valid contract shapes expand and type-check | `cargo check` on a sub-crate |
| `tests/compile-fail-both-features/` | Mutually-exclusive feature gate fires | `cargo check` (asserts failure) |
| `tests/compile-fail-missing-event-trait/` | A registered event type missing its `ContractEvent` impl is rejected | `cargo check` (asserts failure) |

`tests/test-contract/` (workspace member) exercises a single rich shape end-to-end into a WASM build. Fixtures here cover the *variations* that one reference contract does not.

## Topic taxonomy

Both `compile-fail/` and `compile-pass/src/` are nested by topic, not flat:

| Topic dir | Scope |
|---|---|
| `methods/` | Inherent method validation: `validate::public_method`, `validate::new_constructor`, `validate::init_method` |
| `traits/` | Trait method validation: `validate::trait_method` |
| `events/` | Event registration: the `#[contract(events = [...])]` attribute and `parse::events::validate_emitted_types` |
| `directives/` | `#[contract(...)]` directive parsing |
| `feature_gates/` | `contract` / `data-driver` cargo feature enforcement |

Add a new topic dir only when a rule has no reasonable home in the existing ones. Topics are stable categories — they should outlive individual rule renames inside `validate.rs`.

## `// Pins:` header convention

Every fixture starts with a comment naming the rule it pins:

```rust
// Pins: validate::public_method::async
//
// <one-paragraph explanation of why the rule exists>
```

The pin identifier traces back to the function and reject branch in `src/`. Audit "which fixtures pin this rule?" via `grep -r '// Pins: <id>' tests/`.

If you change a rule's identifier (rename a function, restructure a module), `grep` for the old name and update every fixture header in lockstep.

## Adding a compile-fail fixture

Fixtures here depend on the macro crate directly: `use dusk_forge_contract::contract;`. (The compile-pass sub-crate goes through the `dusk_forge::contract` re-export instead — see below.)

1. Pick the topic dir (see above; introduce a new one if none fits).
2. Create `<rule-name>.rs` with:
   - A `// Pins:` header.
   - A 10–25 line minimal repro. Use `pub struct MyContract;` and a `const fn new() -> Self` constructor unless the rule under test requires otherwise.
   - `fn main() {}` at the bottom.
3. Run `TRYBUILD=overwrite cargo test -p dusk-forge-contract --test compile_fail` to bless the `.stderr` companion.
4. Re-run `cargo test -p dusk-forge-contract --test compile_fail` and confirm the new fixture passes.

Earlier checks in `parse::analyze` can shadow your rule — if the diagnostic you see is for a different rule, adjust the fixture so the rule under test fires first.

## Adding a compile-pass fixture

`tests/compile-pass/` is a small library sub-crate (`compile-pass-fixtures`), not a trybuild glob. Each `.rs` file in `src/<topic>/` contains one `#[dusk_forge::contract] pub mod my_contract { … }` that exercises one valid shape. Fixtures here invoke the macro through the `dusk_forge::contract` re-export (the sub-crate depends on `dusk-forge`, not on `dusk-forge-contract` directly).

1. Pick the topic dir under `tests/compile-pass/src/`.
2. Create `<shape-name>.rs` with:
   - A `// Pins:` header naming the canonical valid shape.
   - One `#[dusk_forge::contract] pub mod my_contract { … }`.
   - An `impl Default for MyContract` if the contract carries a `pub fn new()` (avoids `clippy::new_without_default`).
3. Register the file in `src/<topic>/mod.rs` as `pub mod <shape_name>;`.
4. `cargo test -p dusk-forge-contract --test compile_pass`.

`CONTRACT_SCHEMA` is emitted at the parent scope of the macro-annotated module, so one `#[contract]` per file keeps the constants from colliding.

## Why a sub-crate for compile-pass

The macro emits `compile_error!` when neither `contract` nor `data-driver` is enabled in the consuming crate. trybuild has no per-fixture feature configuration, so its `compile_pass` glob cannot enable the feature that the fixtures need. The sub-crate sets `default = ["contract"]`, mirrors the topic structure of `compile-fail/`, and is driven by a single `cargo check` invocation — symmetric with `tests/compile-fail-both-features/`.

## `_dd.rs` (data-driver-js) variants

The plan envisioned `_dd.rs` companion fixtures where the `data-driver-js` feature changes the diagnostic or the accept/reject decision. None are present today: `data-driver-js` is a downstream cargo feature whose only effect is gating `dusk-data-driver/alloc`; it never reaches the macro's parse or validate phases. Add a `_dd.rs` variant only when a future rule starts behaving differently between the modes.
