// Pins: parse::events::module_events::duplicate
//
// Registering the same event type twice in `#[contract(events = [...])]` is
// rejected: a duplicate would emit two identical schema entries and two
// `decode_event` scan blocks. The registered set is unique by construction.

use dusk_forge_contract::contract;

#[contract(events = [events::Foo, events::Foo])]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }
    }
}

fn main() {}
