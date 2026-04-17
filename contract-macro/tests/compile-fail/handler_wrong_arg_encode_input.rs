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

    // encode_input handlers take `&str` — this hands it `&[u8]`, so the
    // macro must reject it before the generated dispatch site fails with a
    // cryptic downstream type error.
    #[contract(encode_input = "raw_id")]
    fn encode_raw_id(bytes: &[u8]) -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error> {
        unimplemented!()
    }
}

fn main() {}
