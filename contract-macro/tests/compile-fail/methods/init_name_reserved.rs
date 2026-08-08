// Pins: parse::functions::public_methods::init_name_reserved
//
// The Rust name `init` is reserved for the WASM deploy export. Post-deploy
// initializers must use another name (e.g. `init_token`).

use dusk_forge_contract::contract;

#[contract]
mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn init(&mut self) {}
    }
}

fn main() {}
