// Pins: validate::new_constructor::has_params
//
// The `new` constructor must take no parameters: it must produce a default
// state with no input, since the host has no way to pass arguments when
// initialising the static STATE.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new(seed: u64) -> Self {
            let _ = seed;
            Self
        }
    }
}

fn main() {}
