// Pins: validate::trait_method::impl_trait_return
//
// A trait method exposed via `#[contract(expose = [...])]` cannot return
// `impl Trait`: the wrapper must serialize a concrete type.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }
    }

    #[contract(expose = [items])]
    impl Collection for MyContract {
        fn items(&self) -> impl Iterator<Item = u64> {
            core::iter::empty()
        }
    }
}

fn main() {}
