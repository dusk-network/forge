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

    // decode_output handlers take exactly one argument; this declares two,
    // so the macro must reject it at expansion time rather than letting the
    // downstream dispatch site fail with an arity mismatch on generated code.
    #[contract(decode_output = "raw_id")]
    fn decode_raw_id(
        rkyv: &[u8],
        extra: &[u8],
    ) -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error> {
        unimplemented!()
    }
}

fn main() {}
