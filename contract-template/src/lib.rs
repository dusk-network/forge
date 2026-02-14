//! Example contract demonstrating the `#[contract]` macro.
//!
//! This is a minimal counter contract showing:
//! - Contract state definition
//! - Public methods (automatically exported)
//! - Event emission

#![no_std]
#![cfg(target_family = "wasm")]

// Require explicit feature selection for WASM builds
#[cfg(not(any(feature = "contract", feature = "data-driver")))]
compile_error!("Enable either 'contract' or 'data-driver' feature for WASM builds");

extern crate alloc;

/// Counter contract with basic increment/decrement functionality.
#[dusk_forge::contract]
mod counter {
    use dusk_core::abi;

    /// Contract state.
    pub struct Counter {
        /// Current count value.
        value: u64,
    }

    impl Counter {
        /// Initialize a new counter with zero.
        pub const fn new() -> Self {
            Self { value: 0 }
        }

        /// Get the current count.
        pub fn get_count(&self) -> u64 {
            self.value
        }

        /// Increment the counter by one.
        pub fn increment(&mut self) {
            let old_value = self.value;
            self.value = self.value.saturating_add(1);
            abi::emit("count_changed", (old_value, self.value));
        }

        /// Decrement the counter by one.
        pub fn decrement(&mut self) {
            let old_value = self.value;
            self.value = self.value.saturating_sub(1);
            abi::emit("count_changed", (old_value, self.value));
        }

        /// Set the counter to a specific value.
        pub fn set_count(&mut self, value: u64) {
            let old_value = self.value;
            self.value = value;
            abi::emit("count_changed", (old_value, self.value));
        }
    }
}
