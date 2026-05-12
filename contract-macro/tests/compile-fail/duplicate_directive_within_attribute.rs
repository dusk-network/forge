// Duplicate directive keys within a single `#[contract(...)]` attribute are
// rejected. This pins the strict parse stance against silent last-wins.

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        #[contract(feeds = "u64", feeds = "u32")]
        pub fn stream(&self) {}
    }
}

fn main() {}
