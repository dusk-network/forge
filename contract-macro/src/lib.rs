// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

#![feature(let_chains)]

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
//!     use evm_core::standard_bridge::SetU64;
//!     use dusk_core::Address;
//!
//!     pub struct MyContract {
//!         value: u64,
//!     }
//!
//!     impl MyContract {
//!         pub fn set_value(&mut self, value: SetU64) {
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
mod extract;
mod generate;
mod parse;
mod resolve;
mod validate;

use proc_macro::TokenStream;
use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse_macro_input, visit::Visit, Attribute, Expr, ExprCall, ExprLit, ExprPath, FnArg,
    ImplItemFn, Item, ItemImpl, ItemMod, Lit, Type,
};

// ============================================================================
// Data Structures
// ============================================================================

/// Information about an imported type.
#[derive(Clone)]
struct ImportInfo {
    /// The short name used in the contract (e.g., `SetU64`).
    name: String,
    /// The full path to the type (e.g., `evm_core::standard_bridge::SetU64`).
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
    /// Whether the parameter is a reference (requires `&` when passing to method).
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
    /// Whether this method has the `#[contract(custom)]` attribute.
    is_custom: bool,
    /// Whether the method returns a reference (requires `.clone()` in wrapper).
    returns_ref: bool,
    /// The method's receiver type (`&self`, `&mut self`, or none).
    receiver: Receiver,
    /// For trait methods with empty bodies: the trait name to call the default impl.
    trait_name: Option<String>,
    /// The type fed via `abi::feed()` for streaming functions (from `#[contract(feeds = "Type")]`).
    /// When present, the data-driver uses this type for `decode_output_fn` instead of `output_type`.
    feed_type: Option<TokenStream2>,
}

/// Information about an event extracted from `abi::emit()` calls.
struct EventInfo {
    /// The event topic string.
    topic: String,
    /// The event data type.
    data_type: TokenStream2,
}

/// Which data-driver method a custom handler implements.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DataDriverRole {
    /// Handles `encode_input_fn` for a data-driver function.
    EncodeInput,
    /// Handles `decode_input_fn` for a data-driver function.
    DecodeInput,
    /// Handles `decode_output_fn` for a data-driver function.
    DecodeOutput,
}

/// Information about a custom data-driver handler function.
struct CustomDataDriverHandler {
    /// The data-driver function name this handler is for (e.g., `"extra_data"`).
    fn_name: String,
    /// Which role this handler plays.
    role: DataDriverRole,
    /// The function item itself (to be moved into `data_driver` module).
    func: syn::ItemFn,
}

/// Visitor to find `abi::emit()` calls within function bodies.
struct EmitVisitor {
    /// Collected events.
    events: Vec<EventInfo>,
}

impl EmitVisitor {
    /// Create a new empty visitor.
    fn new() -> Self {
        Self { events: Vec::new() }
    }
}

impl<'ast> Visit<'ast> for EmitVisitor {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        // Check if this is an abi::emit() call
        if let Expr::Path(ExprPath { path, .. }) = &*node.func {
            let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();

            // Match abi::emit or just emit
            let is_emit = matches!(
                segments
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice(),
                ["abi", "emit"] | ["emit"]
            );

            if is_emit && node.args.len() >= 2 {
                // First arg is the topic - can be a string literal or a const path
                let topic = extract::topic_from_expr(node.args.first().unwrap());

                if let Some(topic) = topic {
                    // Second arg is the event data - extract its type
                    let data_expr = &node.args[1];
                    let data_type = extract::type_from_expr(data_expr);

                    self.events.push(EventInfo { topic, data_type });
                }
            }
        }

        // Continue visiting nested expressions
        syn::visit::visit_expr_call(self, node);
    }
}

/// Visitor to detect `abi::feed()` calls within function bodies.
struct FeedVisitor {
    /// The expressions passed to `abi::feed()` calls, as strings.
    feed_exprs: Vec<String>,
}

impl FeedVisitor {
    /// Create a new visitor.
    fn new() -> Self {
        Self {
            feed_exprs: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for FeedVisitor {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        // Check if this is an abi::feed() call
        if let Expr::Path(ExprPath { path, .. }) = &*node.func {
            let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();

            // Match abi::feed or just feed
            let is_feed = matches!(
                segments
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice(),
                ["abi", "feed"] | ["feed"]
            );

            if is_feed && !node.args.is_empty() {
                // Capture the expression being fed
                let expr = &node.args[0];
                let expr_str = quote!(#expr).to_string();
                self.feed_exprs.push(expr_str);
            }
        }

        // Continue visiting nested expressions
        syn::visit::visit_expr_call(self, node);
    }
}

/// Check if a method body contains `abi::feed()` calls.
/// Returns the expressions being fed (empty if no feed calls).
fn get_feed_exprs(method: &ImplItemFn) -> Vec<String> {
    use syn::visit::Visit;
    let mut visitor = FeedVisitor::new();
    visitor.visit_block(&method.block);
    visitor.feed_exprs
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
    /// Custom data-driver handler functions.
    custom_handlers: Vec<CustomDataDriverHandler>,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Check if an identifier is a relative path keyword.
fn is_relative_path_keyword(ident: &str) -> bool {
    matches!(ident, "self" | "super" | "crate")
}

/// Check if a method body is empty (just `{}`).
///
/// Empty bodies in trait impls signal "use the default implementation,
/// I'm just providing the signature for wrapper generation".
fn has_empty_body(method: &ImplItemFn) -> bool {
    method.block.stmts.is_empty()
}

/// Extract the receiver type from a method signature.
fn extract_receiver(method: &ImplItemFn) -> Receiver {
    if let Some(FnArg::Receiver(receiver)) = method.sig.inputs.first() {
        if receiver.mutability.is_some() {
            Receiver::RefMut
        } else {
            Receiver::Ref
        }
    } else {
        Receiver::None
    }
}

/// Extract doc comments from attributes.
fn extract_doc_comment(attrs: &[Attribute]) -> Option<String> {
    let docs: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc")
                && let syn::Meta::NameValue(meta) = &attr.meta
                && let Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                }) = &meta.value
            {
                return Some(s.value().trim().to_string());
            }
            None
        })
        .collect();

    if docs.is_empty() {
        None
    } else {
        Some(docs.join(" "))
    }
}

/// Check if method has #[contract(custom)] attribute.
fn has_custom_attribute(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("contract") {
            // Parse the attribute arguments
            if let Ok(meta) = attr.meta.require_list() {
                let tokens = meta.tokens.to_string();
                return tokens.contains("custom");
            }
        }
        false
    })
}

/// Extract the `feeds` type from a `#[contract(feeds = "Type")]` attribute.
///
/// This attribute specifies the type fed via `abi::feed()` for streaming functions.
/// When present, the data-driver uses this type for `decode_output_fn` instead of the
/// function's return type.
///
/// Returns `Some(TokenStream2)` with the feed type if found, `None` otherwise.
fn extract_feeds_attribute(attrs: &[Attribute]) -> Option<TokenStream2> {
    for attr in attrs {
        if !attr.path().is_ident("contract") {
            continue;
        }

        let Ok(meta) = attr.meta.require_list() else {
            continue;
        };

        // Parse: feeds = "Type"
        let tokens = meta.tokens.clone();
        let mut iter = tokens.into_iter().peekable();

        // Look for "feeds"
        let Some(proc_macro2::TokenTree::Ident(ident)) = iter.next() else {
            continue;
        };
        if ident != "feeds" {
            continue;
        }

        // Expect "="
        let Some(proc_macro2::TokenTree::Punct(punct)) = iter.next() else {
            continue;
        };
        if punct.as_char() != '=' {
            continue;
        }

        // Expect string literal with type
        let Some(proc_macro2::TokenTree::Literal(lit)) = iter.next() else {
            continue;
        };
        let lit_str = lit.to_string();
        // Remove quotes from the literal
        let type_str = lit_str.trim_matches('"');

        // Parse the type string into tokens
        if let Ok(ty) = syn::parse_str::<syn::Type>(type_str) {
            return Some(quote! { #ty });
        }
    }

    None
}

/// Generate the argument expression for passing to the method.
///
/// For reference parameters, adds `&` or `&mut` prefix.
fn generate_arg_expr(param: &ParameterInfo) -> TokenStream2 {
    let name = &param.name;
    if param.is_mut_ref {
        quote! { &mut #name }
    } else if param.is_ref {
        quote! { &#name }
    } else {
        quote! { #name }
    }
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
/// - The module contains relative imports (`use self::`, `use super::`, `use crate::`)
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
    let data = match extract::contract_data(&module, items) {
        Ok(data) => data,
        Err(e) => return e.to_compile_error().into(),
    };

    let ContractData {
        imports,
        contract_name,
        contract_ident,
        impl_blocks,
        trait_impls,
        custom_handlers,
    } = data;

    // Extract functions and events from all inherent impl blocks
    let mut functions = Vec::new();
    let mut events = Vec::new();

    for impl_block in &impl_blocks {
        match extract::public_methods(impl_block) {
            Ok(methods) => functions.extend(methods),
            Err(e) => return e.to_compile_error().into(),
        }
        events.extend(extract::emit_calls(impl_block));
    }

    // Extract functions and events from trait impl blocks with expose lists
    for trait_impl in &trait_impls {
        match extract::trait_methods(trait_impl) {
            Ok(trait_functions) => functions.extend(trait_functions),
            Err(e) => return e.to_compile_error().into(),
        }
        events.extend(extract::emit_calls(trait_impl.impl_block));
    }

    // Deduplicate events by topic
    let mut seen = std::collections::HashSet::new();
    let events: Vec<_> = events
        .into_iter()
        .filter(|e| seen.insert(e.topic.clone()))
        .collect();

    // Generate schema
    let schema = generate::schema(&contract_name, &imports, &functions, &events);

    // Generate static STATE variable
    let state_static = generate::state_static(&contract_ident);

    // Generate extern "C" wrappers
    let externs = generate::extern_wrappers(&functions, &contract_ident);

    // Build resolved type map for data_driver
    let type_map = resolve::build_type_map(&imports, &functions, &events);

    // Generate data_driver module at crate root level (outside contract module)
    let data_driver = data_driver::module(&type_map, &functions, &events, &custom_handlers);

    // Rebuild the module with stripped contract attributes on methods
    let mod_vis = &module.vis;
    let mod_name = &module.ident;
    let mod_attrs = &module.attrs;

    let new_items: Vec<_> = items
        .iter()
        // Filter out custom data-driver handler functions (they go in the data_driver module)
        .filter(|item| !extract::is_custom_handler(item))
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
