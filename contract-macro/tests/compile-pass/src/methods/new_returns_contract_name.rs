// Pins: validate::new_constructor::valid (return-by-type-name shape)
//
// The `new` constructor may name the contract type explicitly in its
// return type instead of `Self`. `tests/test-contract/` exercises the
// `-> Self` form; this fixture pins the `-> ContractName` form so the
// macro must keep accepting both.

#[dusk_forge::contract]
pub mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> MyContract {
            MyContract
        }
    }

    impl Default for MyContract {
        fn default() -> Self {
            Self::new()
        }
    }
}
