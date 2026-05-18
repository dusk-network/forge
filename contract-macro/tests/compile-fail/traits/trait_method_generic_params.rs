// Pins: validate::trait_method::generic_params
//
// A trait method exposed via `#[contract(expose = [...])]` cannot carry
// generic type parameters: the generated extern "C" wrapper requires
// concrete types.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }
    }

    #[contract(expose = [process])]
    impl Processor for MyContract {
        fn process<T>(&self, value: T) {
            let _ = value;
        }
    }
}

fn main() {}
