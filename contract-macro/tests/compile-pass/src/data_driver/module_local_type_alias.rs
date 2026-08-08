// Pins: generate::contract_module::module_local_const_generic
//
// `pub type` alias whose definition uses a module-local `const` (forge#36).

#[dusk_forge::contract]
pub mod my_contract {
    const DEPTH: usize = 12;

    pub type DepthBuf = [u8; DEPTH];

    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn depth_buf(&self) -> DepthBuf {
            [0; DEPTH]
        }
    }
}
