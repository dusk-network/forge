// Pins: validate::public_method::async
//
// A public inherent method declared `async` must be rejected: WASM contracts
// run synchronously inside the host VM and have no executor to drive futures.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub async fn fetch(&self) -> u64 {
            0
        }
    }
}

fn main() {}
