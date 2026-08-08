// Pins: validate::init_method::returns_value
//
// A `#[contract(init)]` deploy constructor must return `()`.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        #[contract(init)]
        pub fn initialize(&mut self, seed: u64) -> bool {
            let _ = seed;
            true
        }
    }
}

fn main() {}
