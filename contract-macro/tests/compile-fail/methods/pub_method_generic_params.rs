// Pins: validate::public_method::generic_params
//
// A public inherent method with generic type parameters must be rejected:
// extern "C" wrappers require concrete types, so generics cannot survive
// monomorphisation into the WASM export surface.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn process<T>(&self, value: T) -> T {
            value
        }
    }
}

fn main() {}
