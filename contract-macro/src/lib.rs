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
use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::quote;
use syn::{Item, ItemImpl, ItemMod, Type, parse_macro_input};

// ============================================================================
// IR Data Structures
// ============================================================================

/// Information about an imported type.
#[derive(Clone)]
struct ImportInfo {
    /// The short name used in the contract (e.g., `SetU64`).
    name: String,
    /// The full path to the type (e.g., `my_crate::MyType`).
    path: String,
}

/// The receiver type of a method (self parameter).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Receiver {
    /// No receiver - associated function.
    None,
    /// Immutable borrow: `&self`.
    Ref,
    /// Mutable borrow: `&mut self`.
    RefMut,
}

/// Information about a function parameter.
struct ParameterInfo {
    /// The parameter name.
    name: Ident,
    /// The type (dereferenced if the parameter is a reference).
    ty: TokenStream2,
    /// Whether the parameter is a reference (requires `&` when passing to
    /// method).
    is_ref: bool,
    /// Whether the parameter is a mutable reference.
    is_mut_ref: bool,
}

/// Information about a contract function extracted from the impl block.
struct FunctionInfo {
    /// The function name.
    name: Ident,
    /// Documentation comment.
    doc: Option<String>,
    /// Function parameters.
    params: Vec<ParameterInfo>,
    /// The input type (tuple of parameter types or single type).
    input_type: TokenStream2,
    /// The output type (dereferenced if the method returns a reference).
    output_type: TokenStream2,
    /// Whether the method returns a reference (requires `.clone()` in wrapper).
    returns_ref: bool,
    /// The method's receiver type (`&self`, `&mut self`, or none).
    receiver: Receiver,
    /// For trait methods with empty bodies: the trait name to call the default
    /// impl.
    trait_name: Option<String>,
    /// The type fed via `abi::feed()` for streaming functions (from
    /// `#[contract(feeds = "Type")]`). When present, the data-driver uses
    /// this type for `decode_output_fn` instead of `output_type`.
    feed_type: Option<TokenStream2>,
}

/// Information about an event extracted from `abi::emit()` calls.
#[derive(Clone)]
struct EventInfo {
    /// The event topic string.
    topic: String,
    /// The event data type.
    data_type: TokenStream2,
}

/// Result of extracting imports from a use statement.
struct ImportExtraction {
    /// The extracted imports.
    imports: Vec<ImportInfo>,
    /// Whether a glob import was found.
    has_glob: bool,
    /// Whether a relative import was found.
    has_relative: bool,
}

/// Information about a trait implementation with exposed methods.
struct TraitImplInfo<'a> {
    /// The name of the trait being implemented (for error messages).
    trait_name: String,
    /// The impl block itself.
    impl_block: &'a ItemImpl,
    /// List of method names to expose (from `#[contract(expose = [...])]`).
    expose_list: Vec<String>,
}

/// Validated contract module data extracted during parsing.
struct ContractData<'a> {
    /// Imported types.
    imports: Vec<ImportInfo>,
    /// The contract struct name as a string.
    contract_name: String,
    /// The contract struct identifier.
    contract_ident: Ident,
    /// Inherent impl blocks for the contract.
    impl_blocks: Vec<&'a ItemImpl>,
    /// Trait implementations with `#[contract(expose = [...])]` attributes.
    trait_impls: Vec<TraitImplInfo<'a>>,
}

// ============================================================================
// Main Macro
// ============================================================================

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
pub fn contract(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let module = parse_macro_input!(item as ItemMod);

    // Module must have content (not just a declaration)
    let Some((_, items)) = &module.content else {
        return syn::Error::new_spanned(&module, "#[contract] requires a module with content")
            .to_compile_error()
            .into();
    };

    // Validate and extract contract data
    let data = match parse::contract_data(&module, items) {
        Ok(data) => data,
        Err(e) => return e.to_compile_error().into(),
    };

    let ContractData {
        imports,
        contract_name,
        contract_ident,
        impl_blocks,
        trait_impls,
    } = data;

    // Extract functions and events from all inherent impl blocks
    let mut functions = Vec::new();
    let mut events = Vec::new();

    for impl_block in &impl_blocks {
        match parse::public_methods(impl_block) {
            Ok(methods) => functions.extend(methods),
            Err(e) => return e.to_compile_error().into(),
        }
        events.extend(parse::emit_calls(impl_block));
        // Include events from method-level #[contract(emits = [...])] attributes
        events.extend(parse::inherent_method_emits(impl_block));
    }

    // Extract functions and events from trait impl blocks with expose lists
    for trait_impl in &trait_impls {
        match parse::trait_methods(trait_impl) {
            Ok(trait_functions) => functions.extend(trait_functions),
            Err(e) => return e.to_compile_error().into(),
        }
        events.extend(parse::emit_calls(trait_impl.impl_block));
        // Include events from method-level #[contract(emits = [...])] attributes
        events.extend(parse::trait_method_emits(trait_impl));
    }

    // Deduplicate events by topic — first-seen wins.
    let events = parse::dedup_events_by_topic(events);

    // Generate schema
    let schema = generate::schema(&contract_name, &imports, &functions, &events);

    // Generate static STATE variable
    let state_static = generate::state_static(&contract_ident);

    // Generate extern "C" wrappers
    let externs = generate::extern_wrappers(&functions, &contract_ident);

    // Build resolved type map for data_driver
    let type_map = resolve::build_type_map(&imports, &functions, &events);

    // Generate data_driver module at crate root level (outside contract module)
    let data_driver = data_driver::module(&type_map, &functions, &events);

    // Rebuild the module with stripped contract attributes on methods
    let mod_vis = &module.vis;
    let mod_name = &module.ident;
    let mod_attrs = &module.attrs;

    let new_items: Vec<_> = items
        .iter()
        .map(|item| {
            if let Item::Impl(impl_block) = item
                && let Type::Path(type_path) = &*impl_block.self_ty
                && type_path.path.is_ident(&contract_name)
            {
                // Strip #[contract(...)] attributes from both inherent and trait impl blocks
                Item::Impl(generate::strip_contract_attributes(impl_block.clone()))
            } else {
                item.clone()
            }
        })
        .collect();

    // Output:
    // - Contract schema at crate root (always available)
    // - Contract module wrapped in #[cfg(not(feature = "data-driver"))]
    // - Data driver module at crate root with #[cfg(feature = "data-driver")]
    let output = quote! {
        #[cfg(not(any(feature = "contract", feature = "data-driver")))]
        compile_error!("Enable either 'contract' or 'data-driver' feature for WASM builds");

        #[cfg(all(feature = "contract", feature = "data-driver"))]
        compile_error!("Features 'contract' and 'data-driver' are mutually exclusive");

        #[cfg(any(feature = "contract", feature = "data-driver"))]
        #schema

        #[cfg(not(feature = "data-driver"))]
        #(#mod_attrs)*
        #mod_vis mod #mod_name {
            #(#new_items)*

            #state_static

            #externs
        }

        #data_driver
    };

    output.into()
}
