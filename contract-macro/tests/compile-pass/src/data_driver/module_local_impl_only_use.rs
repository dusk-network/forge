// Pins: generate::data_visible_items::skips_impl_only_use
//
// `use` lines referenced only from impl bodies must not appear in the
// data-visible module (forge#36). Including `use only_impl::abi_helper` would
// fail because `only_impl` is not copied into that cfg branch.

#[dusk_forge::contract]
pub mod my_contract {
    mod only_impl {
        pub fn abi_helper() -> u32 {
            42
        }
    }

    use only_impl::abi_helper;

    const DEPTH: usize = 8;

    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }

        pub fn touch_impl_only_use(&mut self) {
            let _ = abi_helper();
        }

        pub fn depth_bytes(&self) -> [u8; DEPTH] {
            [0; DEPTH]
        }
    }
}
