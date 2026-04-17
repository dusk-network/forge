use dusk_forge_contract::contract;

#[contract]
mod my_contract {
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

    // The generated dispatcher calls this handler with a local-lifetime
    // borrow of the incoming JSON — a handler that promises `'static` can't
    // bind it. The validator must reject at the signature site rather than
    // letting a lifetime mismatch surface inside macro-generated code the
    // user didn't write.
    #[contract(encode_input = "raw_id")]
    fn encode_raw_id(json: &'static str) -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error> {
        unimplemented!()
    }
}

fn main() {}
