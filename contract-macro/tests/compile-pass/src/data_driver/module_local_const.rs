// Pins: generate::contract_module::module_local_const_generic
//
// Module-local `const` as a const-generic argument in a public signature under
// the `data-driver` feature (forge#36). Resolves `DEPTH` in `[u8; DEPTH]` and
// compiles the const in the data-visible module copy.

#[dusk_forge::contract]
pub mod my_contract {
    const DEPTH: usize = 8;

    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn depth_bytes(&self) -> [u8; DEPTH] {
            [0; DEPTH]
        }
    }
}
