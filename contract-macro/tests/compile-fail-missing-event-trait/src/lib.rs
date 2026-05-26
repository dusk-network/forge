// Pins: generate::schema::contract_event_bound
//
// A type listed in `#[contract(events = [...])]` must implement
// `dusk_forge::ContractEvent`. The generated `CONTRACT_SCHEMA` reads its
// `TOPICS` const, so a missing impl is rejected as an unsatisfied trait bound.
//
// This lives in its own sub-crate (not a trybuild fixture) because the error
// surfaces only when the `contract` feature is enabled — the schema is
// feature-gated, and trybuild has no per-fixture feature configuration. The
// harness in `compile_fail.rs` asserts this crate fails to build.

#![no_std]

pub struct NotAnEvent;

#[dusk_forge::contract(events = [crate::NotAnEvent])]
pub mod my_contract {
    pub struct MyContract;

    impl MyContract {
        pub const fn new() -> Self {
            Self
        }
    }
}
