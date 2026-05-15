// Pins: validate::init_method::absent_ok
//
// A contract without an `init` method must compile: `init` is optional,
// the host calls it only when present. `tests/test-contract/` defines an
// `init`; this fixture pins the no-`init` shape.

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
