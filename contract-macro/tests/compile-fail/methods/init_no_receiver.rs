// Pins: validate::init_method::no_receiver
//
// An `init` method declared as an associated function (no `self`) must be
// rejected: initialisation needs access to contract state through `&mut
// self`.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn init(seed: u64) {
            let _ = seed;
        }
    }
}

fn main() {}
