// Pins: validate::init_method::absent_ok
//
// A deploy constructor is optional: contracts without `#[contract(init)]` must
// compile. `tests/test-contract/` uses `#[contract(init)] fn initialize`; this
// fixture pins the no-deploy-constructor shape.

#[dusk_forge::contract]
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
