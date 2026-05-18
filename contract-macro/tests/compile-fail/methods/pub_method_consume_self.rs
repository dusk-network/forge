// Pins: validate::public_method::consume_self
//
// A public inherent method consuming `self` must be rejected: the static
// STATE singleton cannot be moved out, only borrowed via `&self` / `&mut
// self`.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn destroy(self) {}
    }
}

fn main() {}
