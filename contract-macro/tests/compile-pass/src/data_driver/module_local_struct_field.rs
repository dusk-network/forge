// Pins: generate::contract_module::module_local_const_generic
//
// Struct field type uses a module-local `const` as array length (forge#36).

#[dusk_forge::contract]
pub mod my_contract {
    const DEPTH: usize = 4;

    pub struct MyContract {
        buf: [u8; DEPTH],
    }

    impl MyContract {
        pub const fn new() -> Self {
            Self { buf: [0; DEPTH] }
        }

        pub fn buf(&self) -> &[u8; DEPTH] {
            &self.buf
        }
    }
}
