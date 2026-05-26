// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Procedural macro for the `#[contract]` attribute.
//!
//! This macro is applied to a module containing a contract struct and its
//! impl block. It extracts metadata about public methods and events, and
//! generates a `CONTRACT_SCHEMA` constant plus extern "C" wrappers.
//!
//! # Pipeline
//!
//! 1. [`parse::analyze`] walks the user module and produces an
//!    [`parse::Analysis`] (functions, registered events, imports, contract
//!    identifier).
//! 2. [`generate`] / [`data_driver`] consume the analysis and emit the contract
//!    or data-driver bindings.
//!
//! `lib.rs` only orchestrates these two phases — all walking, validation,
//! and IR construction lives in the `parse` module.
//!
//! # Example
//!
//! ```ignore
//! #[contract]
//! mod my_contract {
//!     use my_crate::MyType;
//!     use dusk_core::abi;
//!
//!     pub struct MyContract {
//!         value: u64,
//!     }
//!
//!     impl MyContract {
//!         pub fn set_value(&mut self, value: MyType) {
//!             // ...
//!         }
//!     }
//! }
//! ```

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(unused_must_use)]
#![deny(unused_extern_crates)]
#![deny(clippy::pedantic)]
#![warn(missing_debug_implementations, unreachable_pub, rustdoc::all)]

mod data_driver;
mod generate;
mod parse;
mod resolve;
mod validate;

use proc_macro::TokenStream;
use syn::{ItemMod, parse_macro_input};

/// The main contract proc macro.
///
/// Applied to a module containing a contract struct and impl block.
/// Extracts metadata and generates schema + extern wrappers.
///
/// # Errors
///
/// This macro will produce compile errors if:
/// - The module has no content (just a declaration like `mod foo;`)
/// - The module contains glob imports (`use foo::*`)
/// - The module contains relative imports (`use self::`, `use super::`, `use
///   crate::`)
/// - The module contains multiple `pub struct` declarations
/// - The module contains no `pub struct`
/// - The module contains no impl block for the contract struct
/// - A public method has no `self` receiver (associated functions)
/// - A public method has generic type or const parameters
/// - A public method is async
/// - A public method consumes `self` instead of borrowing it
/// - A public method uses `impl Trait` in parameters or return type
#[proc_macro_attribute]
pub fn contract(attr: TokenStream, item: TokenStream) -> TokenStream {
    let module = parse_macro_input!(item as ItemMod);

    let registered_events = match parse::events::module_events(attr.into()) {
        Ok(events) => events,
        Err(e) => return e.to_compile_error().into(),
    };

    let Some((_, items)) = &module.content else {
        return syn::Error::new_spanned(&module, "#[contract] requires a module with content")
            .to_compile_error()
            .into();
    };

    let analysis = match parse::analyze(&module, items, &registered_events) {
        Ok(analysis) => analysis,
        Err(e) => return e.to_compile_error().into(),
    };

    generate::contract_module(&module, items, &analysis).into()
}
