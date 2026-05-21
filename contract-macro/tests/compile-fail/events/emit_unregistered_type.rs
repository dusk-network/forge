// Pins: parse::events::validate_emitted_types::unregistered
//
// An `abi::emit()` call inside the contract module whose data type is not in
// the `#[contract(events = [...])]` list must be rejected, with the diagnostic
// anchored at the offending emit. This pins the validator's core contract:
// the registered list is the single source of truth, and in-module emits are
// checked against it.

use dusk_forge_contract::contract;

#[contract(events = [Registered])]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn act(&mut self) {
            abi::emit("topic", Unregistered {});
        }
    }
}

fn main() {}
