// Pins: validate::method_emits_event::mut_self_no_emit
//
// A public `&mut self` method that neither calls `abi::emit()` in its body,
// nor registers events via `#[contract(emits = [...])]`, nor opts out via
// `#[contract(no_event)]`, must be rejected: state-mutating methods are
// required to be observable.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn touch(&mut self) {}
    }
}

fn main() {}
