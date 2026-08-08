// Pins: generate::extern_wrappers::deploy_constructor_exports_as_init
//
// `#[contract(init)]` on `initialize` still exports the WASM symbol `init` while
// leaving `initialize` callable as a normal post-deploy method name in source.

#[dusk_forge::contract]
pub mod my_contract {
    pub struct MyContract {
        value: u64,
    }

    impl MyContract {
        pub const fn new() -> Self {
            Self { value: 0 }
        }

        #[contract(init)]
        pub fn initialize(&mut self, value: u64) {
            self.value = value;
        }

        /// Post-deploy name is not `init` — safe for token-style wrappers.
        pub fn init_token(&mut self, value: u64) {
            self.value = value;
        }
    }

    impl Default for MyContract {
        fn default() -> Self {
            Self::new()
        }
    }
}
