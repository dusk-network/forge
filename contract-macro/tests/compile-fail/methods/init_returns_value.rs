// Pins: validate::init_method::returns_value
//
// An `init` method that returns a non-unit type must be rejected:
// initialisation has no caller to consume a return value, so errors must
// panic instead.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn init(&mut self) -> bool {
            true
        }
    }
}

fn main() {}
