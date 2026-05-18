// Pins: validate::trait_method::impl_trait_param
//
// A trait method exposed via `#[contract(expose = [...])]` cannot use
// `impl Trait` in a parameter: the wrapper needs a concrete type.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }
    }

    #[contract(expose = [run])]
    impl Runner for MyContract {
        fn run(&self, handler: impl core::fmt::Display) {
            let _ = handler;
        }
    }
}

fn main() {}
