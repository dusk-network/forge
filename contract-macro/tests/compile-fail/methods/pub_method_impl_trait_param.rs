// Pins: validate::public_method::impl_trait_param
//
// A public inherent method using `impl Trait` in a parameter must be
// rejected: extern "C" wrappers need a concrete monomorphic type to
// deserialize into.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn process(&self, handler: impl core::fmt::Display) {
            let _ = handler;
        }
    }
}

fn main() {}
