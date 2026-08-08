// Pins: generate::contract_module::module_local_type_alias
//
// Private module-local type alias in a public method signature (forge#36).

#[dusk_forge::contract]
pub mod my_contract {
    type Byte = u8;

    const DEPTH: usize = 6;

    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn depth_bytes(&self) -> [Byte; DEPTH] {
            [0; DEPTH]
        }
    }
}
