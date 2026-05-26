// Pins: parse::events::module_attribute_path_list
//
// The `#[contract(events = [...])]` module attribute accepts a list of event
// type paths; each must implement `dusk_forge::ContractEvent`. This pins that
// the attribute parses, that a single-topic and a multi-topic event coexist,
// and that the generated `CONTRACT_SCHEMA` type-checks against the trait's
// `TOPICS` const. `tests/test-contract/` exercises the runtime path; this
// fixture pins the minimal accept shape.

use dusk_forge::ContractEvent;

pub struct Toggled;

impl ContractEvent for Toggled {
    const TOPICS: &'static [&'static str] = &["toggled"];
}

pub struct Lifecycle;

impl ContractEvent for Lifecycle {
    const TOPICS: &'static [&'static str] = &["created", "destroyed"];
}

#[dusk_forge::contract(events = [
    crate::events::registered_events::Toggled,
    crate::events::registered_events::Lifecycle,
])]
pub mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }
    }

    impl Default for MyContract {
        fn default() -> Self {
            Self::new()
        }
    }
}
