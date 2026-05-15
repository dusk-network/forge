// Pins: generate::feature_gate::missing
//
// Applying `#[contract]` without enabling either the `contract` or the
// `data-driver` cargo feature fires the generated `compile_error!` that
// guards against unconfigured WASM builds. The contract body is shared
// with `compile-fail-both-features/` via `include!`.

include!("../../common/contract.rs");

fn main() {}
