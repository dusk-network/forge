// Pins: validate::init_method::no_receiver
//
// A `#[contract(init)]` deploy constructor declared as an associated function
// (no `self`) must be rejected.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        #[contract(init)]
        pub fn initialize(seed: u64) {
            let _ = seed;
        }
    }
}

fn main() {}
