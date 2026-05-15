// Pins: validate::init_method::immutable_self
//
// An `init` method with an immutable `&self` receiver must be rejected:
// initialisation has to mutate the contract state.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn init(&self) {}
    }
}

fn main() {}
