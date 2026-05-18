// Pins: validate::init_method::consume_self
//
// A private `init` method that consumes `self` must be rejected by the
// init-specific check. (The public flavour fires the broader
// `public_method::consume_self` rule first; this fixture keeps `init`
// private so the init-specific path is exercised.)

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        fn init(self) {}
    }
}

fn main() {}
