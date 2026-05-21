// Pins: validate::init_method::no_params_ok
//
// An `init` method declared as `&mut self` with no additional parameters
// must compile. `tests/test-contract/` exercises `init(&mut self, owner)`;
// this fixture pins the parameter-less variant.

#[dusk_forge::contract]
pub mod my_contract {
    pub struct MyContract {
        initialized: bool,
    }

    impl MyContract {
        pub const fn new() -> Self {
            Self { initialized: false }
        }

        pub fn init(&mut self) {
            self.initialized = true;
        }
    }

    impl Default for MyContract {
        fn default() -> Self {
            Self::new()
        }
    }
}
