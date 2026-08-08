// Pins: validate::init_method::valid_no_params
//
// A deploy constructor marked with `#[contract(init)]` may take only `&mut self`.

#[dusk_forge::contract]
pub mod my_contract {
    pub struct MyContract {
        initialized: bool,
    }

    impl MyContract {
        pub const fn new() -> Self {
            Self { initialized: false }
        }

        #[contract(init)]
        pub fn initialize(&mut self) {
            self.initialized = true;
        }
    }

    impl Default for MyContract {
        fn default() -> Self {
            Self::new()
        }
    }
}
