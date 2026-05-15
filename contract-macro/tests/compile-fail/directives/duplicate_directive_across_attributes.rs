// Pins: parse::directives::duplicate_across_attributes
//
// Duplicate directive keys across multiple `#[contract(...)]` attributes on
// the same item are rejected. This pins the strict parse stance against
// silent last-attr-wins.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        #[contract(feeds = "u64")]
        #[contract(feeds = "u32")]
        pub fn stream(&self) {}
    }
}

fn main() {}
