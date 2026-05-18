// Pins: validate::new_constructor::wrong_return
//
// The `new` constructor must return `Self` (or the contract type name);
// any other return type makes the generated `static mut STATE` initialiser
// type-incompatible.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> u64 {
            0
        }
    }
}

fn main() {}
