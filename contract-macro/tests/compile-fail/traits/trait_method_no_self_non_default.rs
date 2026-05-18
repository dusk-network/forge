// Pins: validate::trait_method::no_self_non_default
//
// A trait method exposed via `#[contract(expose = [...])]` with a non-empty
// body (a non-default impl) must take a `self` receiver. Associated
// functions are only allowed when the body is empty `{}`, signalling
// "delegate to the trait default".

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }
    }

    #[contract(expose = [version])]
    impl Versioned for MyContract {
        fn version() -> &'static str {
            "1.0"
        }
    }
}

fn main() {}
