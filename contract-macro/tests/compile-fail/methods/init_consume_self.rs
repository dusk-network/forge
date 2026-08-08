// Pins: validate::init_method::consume_self
//
// A `#[contract(init)]` deploy constructor that consumes `self` must be rejected.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        #[contract(init)]
        pub fn initialize(self, seed: u64) {}
    }
}

fn main() {}
