// Pins: validate::new_constructor::missing
//
// A contract module without a `const fn new() -> Self` method must be
// rejected: the static STATE singleton is initialised via that constructor.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub fn get(&self) -> u64 {
            0
        }
    }
}

fn main() {}
