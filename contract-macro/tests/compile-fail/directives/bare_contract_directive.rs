// Pins: parse::directives::bare_attribute
//
// `#[contract]` without a directive list on an inner item is rejected. This
// pins the strict parse stance: every inner `#[contract(...)]` attribute must
// be a list, so a future refactor that loosened this check would trip this
// fixture.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        #[contract]
        pub fn touch(&mut self) {}
    }
}

fn main() {}
