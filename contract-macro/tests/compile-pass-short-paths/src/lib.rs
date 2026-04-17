#![no_std]

// End-to-end coverage for handler re-emit: every failure mode Defect 3
// produced had the validator accept short paths that the splicer
// couldn't actually resolve in the generated submodule. This fixture
// pins the round-trip — if the splicer regresses, `cargo check` fails
// here with `cannot find type …` or a missing-method error, not a
// downstream integration test surprise.
//
// The handlers below reference `Vec`, `Error`, and `JsonValue` as
// short paths in both their signatures *and* their bodies. With
// `data-driver` enabled (the crate's default), the `#[contract]` macro
// splices them into the generated `data_driver` submodule; re-emit
// carries the imports along so the short paths resolve there.

extern crate alloc;

#[dusk_forge::contract]
mod my_contract {
    extern crate alloc;

    use alloc::string::String;
    #[cfg(feature = "data-driver")]
    use alloc::vec::Vec;
    #[cfg(feature = "data-driver")]
    use dusk_data_driver::{Error, JsonValue};

    pub struct MyContract {
        value: u64,
    }

    impl MyContract {
        pub const fn new() -> Self {
            Self { value: 0 }
        }

        pub fn get_value(&self) -> u64 {
            self.value
        }
    }

    #[contract(encode_input = "raw_id")]
    fn encode_raw_id(_json: &str) -> Result<Vec<u8>, Error> {
        // Body references the imported short paths too — re-emit must
        // cover the body, not just the signature.
        Err(Error::Unsupported(String::new()))
    }

    #[contract(decode_output = "raw_id")]
    fn decode_raw_id(_bytes: &[u8]) -> Result<JsonValue, Error> {
        Err(Error::Unsupported(String::new()))
    }
}
