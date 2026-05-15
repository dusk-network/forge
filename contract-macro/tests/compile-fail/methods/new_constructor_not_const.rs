// Pins: validate::new_constructor::not_const
//
// The `new` constructor must be `const fn`: it initialises a `static mut`
// in the generated WASM module, which only `const` evaluation can produce.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub fn new() -> Self {
            Self
        }
    }
}

fn main() {}
