// Pins: validate::trait_method::consume_self
//
// A trait method exposed via `#[contract(expose = [...])]` cannot consume
// `self`: the static STATE singleton can only be borrowed.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }
    }

    #[contract(expose = [destroy])]
    impl Destructible for MyContract {
        fn destroy(self) {}
    }
}

fn main() {}
