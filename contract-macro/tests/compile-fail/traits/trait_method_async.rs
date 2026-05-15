// Pins: validate::trait_method::async
//
// A trait method exposed via `#[contract(expose = [...])]` cannot be
// declared `async`: WASM contracts run synchronously with no executor.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }
    }

    #[contract(expose = [fetch])]
    impl AsyncTrait for MyContract {
        async fn fetch(&self) -> u64 {
            0
        }
    }
}

fn main() {}
