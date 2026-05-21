// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Parse-phase IR: typed data the parse pipeline produces and the generate
//! phase consumes.
//!
//! Everything here is owned (no borrows into the user's `ItemMod`) except
//! [`TraitImplInfo`], which carries a reference to the original impl block so
//! the function extractor can read its items.

use proc_macro2::{Ident, TokenStream as TokenStream2};
use syn::ItemImpl;

/// Information about an imported type.
#[derive(Clone)]
pub(crate) struct ImportInfo {
    /// The short name used in the contract (e.g., `SetU64`).
    pub name: String,
    /// The full path to the type (e.g., `my_crate::MyType`).
    pub path: String,
}

/// The receiver type of a method (self parameter).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Receiver {
    /// No receiver - associated function.
    None,
    /// Immutable borrow: `&self`.
    Ref,
    /// Mutable borrow: `&mut self`.
    RefMut,
}

/// Information about a function parameter.
pub(crate) struct ParameterInfo {
    /// The parameter name.
    pub name: Ident,
    /// The type (dereferenced if the parameter is a reference).
    pub ty: TokenStream2,
    /// Whether the parameter is a reference (requires `&` when passing to
    /// method).
    pub is_ref: bool,
    /// Whether the parameter is a mutable reference.
    pub is_mut_ref: bool,
}

/// Information about a contract function extracted from the impl block.
pub(crate) struct FunctionInfo {
    /// The function name.
    pub name: Ident,
    /// Documentation comment.
    pub doc: Option<String>,
    /// Function parameters.
    pub params: Vec<ParameterInfo>,
    /// The input type (tuple of parameter types or single type).
    pub input_type: TokenStream2,
    /// The output type (dereferenced if the method returns a reference).
    pub output_type: TokenStream2,
    /// Whether the method returns a reference (requires `.clone()` in wrapper).
    pub returns_ref: bool,
    /// The method's receiver type (`&self`, `&mut self`, or none).
    pub receiver: Receiver,
    /// For trait methods with empty bodies: the trait name to call the default
    /// impl.
    pub trait_name: Option<String>,
    /// The type fed via `abi::feed()` for streaming functions (from
    /// `#[contract(feeds = "Type")]`). When present, the data-driver uses
    /// this type for `decode_output_fn` instead of `output_type`.
    pub feed_type: Option<TokenStream2>,
}

/// A contract event registered via the `#[contract(events = [...])]` module
/// attribute.
#[derive(Clone)]
pub(crate) struct EventInfo {
    /// The registered event data type path, exactly as written in the
    /// `events = [...]` list.
    pub data_type: TokenStream2,
}

/// Information about a trait implementation with exposed methods.
///
/// Holds a reference into the original `ItemMod`, so it doesn't outlive the
/// parse phase.
pub(crate) struct TraitImplInfo<'a> {
    /// The name of the trait being implemented (for error messages).
    pub trait_name: String,
    /// The impl block itself.
    pub impl_block: &'a ItemImpl,
    /// List of method names to expose (from `#[contract(expose = [...])]`).
    pub expose_list: Vec<String>,
}

/// The fully-extracted result of analyzing a contract module.
///
/// Returned by [`super::analyze`] and consumed by the `generate` phase.
pub(crate) struct Analysis {
    /// The contract struct identifier.
    pub contract_ident: Ident,
    /// The contract struct name (string form, used in error messages and
    /// schema).
    pub contract_name: String,
    /// Imported types.
    pub imports: Vec<ImportInfo>,
    /// All public methods exposed by the contract — both inherent and
    /// trait-default — flattened into one list.
    pub functions: Vec<FunctionInfo>,
    /// Events registered via the `#[contract(events = [...])]` attribute.
    pub events: Vec<EventInfo>,
}
