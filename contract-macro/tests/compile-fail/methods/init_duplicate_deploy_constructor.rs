// Pins: validate::init_method::duplicate_deploy_constructor
//
// Only one method may carry `#[contract(init)]` per contract.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        #[contract(init)]
        pub fn setup_a(&mut self) {}

        #[contract(init)]
        pub fn setup_b(&mut self) {}
    }
}

fn main() {}
