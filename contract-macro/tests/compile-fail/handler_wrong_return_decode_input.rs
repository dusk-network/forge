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

    // decode_input handlers must return `Result<JsonValue, Error>` — this
    // returns a raw byte vector, which is the wrong role's shape. The
    // validator must name the decode_input role (not a generic error) so
    // the user knows which of their handlers to fix.
    #[contract(decode_input = "raw_id")]
    fn decode_raw_id(rkyv: &[u8]) -> alloc::vec::Vec<u8> {
        unimplemented!()
    }
}

fn main() {}
