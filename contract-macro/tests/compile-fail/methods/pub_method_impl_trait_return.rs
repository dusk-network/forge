// Pins: validate::public_method::impl_trait_return
//
// A public inherent method returning `impl Trait` must be rejected: the
// extern "C" wrapper must serialize a concrete type, not an opaque one.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn iter(&self) -> impl Iterator<Item = u64> {
            core::iter::empty()
        }
    }
}

fn main() {}
