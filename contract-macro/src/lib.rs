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

use proc_macro::TokenStream;
use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Attribute, Expr, ExprCall, ExprLit, ExprPath, FnArg, ImplItem,
    ImplItemFn, Item, ItemImpl, ItemMod, ItemUse, Lit, Pat, ReturnType, Type, UseTree,
    Visibility, visit::Visit,
};

/// Information about an imported type.
#[derive(Clone)]
struct ImportInfo {
    /// The short name used in the contract (e.g., `SetU64`).
    name: String,
    /// The full path to the type (e.g., `evm_core::standard_bridge::SetU64`).
    path: String,
}

/// Information about a function parameter.
struct ParameterInfo {
    name: Ident,
    ty: TokenStream2,
}

/// Information about a contract function extracted from the impl block.
struct FunctionInfo {
    name: Ident,
    doc: Option<String>,
    params: Vec<ParameterInfo>,
    input_type: TokenStream2,
    output_type: TokenStream2,
    is_custom: bool,
}

/// Information about an event extracted from `abi::emit()` calls.
struct EventInfo {
    topic: String,
    data_type: TokenStream2,
}

/// Visitor to find `abi::emit()` calls within function bodies.
struct EmitVisitor {
    events: Vec<EventInfo>,
}

impl EmitVisitor {
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
                segments.iter().map(String::as_str).collect::<Vec<_>>().as_slice(),
                ["abi", "emit"] | ["emit"]
            );

            if is_emit && node.args.len() >= 2 {
                // First arg is the topic - can be a string literal or a const path
                let topic = extract_topic_from_expr(node.args.first().unwrap());

                if let Some(topic) = topic {
                    // Second arg is the event data - extract its type
                    let data_expr = &node.args[1];
                    let data_type = extract_type_from_expr(data_expr);

                    self.events.push(EventInfo { topic, data_type });
                }
            }
        }

        // Continue visiting nested expressions
        syn::visit::visit_expr_call(self, node);
    }
}

/// Extract topic string from the first argument of `abi::emit()`.
/// Handles both string literals and const path expressions.
fn extract_topic_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        // String literal: "topic_name"
        Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) => Some(s.value()),
        // Path expression: Type::TOPIC or module::Type::TOPIC
        Expr::Path(path) => {
            // Convert the path to a string representation
            Some(
                path.path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            )
        }
        _ => None,
    }
}

/// Attempt to extract a type from an expression.
/// This handles common patterns like `Type { .. }`, `Type()`, `Type::new()`.
fn extract_type_from_expr(expr: &Expr) -> TokenStream2 {
    match expr {
        // Handle struct instantiation: events::PauseToggled { ... } or PauseToggled { ... }
        Expr::Struct(s) => {
            let path = &s.path;
            quote! { #path }
        }
        // Handle unit struct or tuple struct: events::PauseToggled() or PauseToggled()
        Expr::Call(call) => {
            if let Expr::Path(path) = &*call.func {
                let p = &path.path;
                quote! { #p }
            } else {
                quote! { () }
            }
        }
        // Handle path expressions: events::PauseToggled
        Expr::Path(path) => {
            let p = &path.path;
            quote! { #p }
        }
        // Fallback - unknown type
        _ => quote! { () },
    }
}

/// Result of extracting imports from a use statement.
struct ImportExtraction {
    imports: Vec<ImportInfo>,
    has_glob: bool,
    has_relative: bool,
}

/// Extract imports from a `use` statement.
fn extract_imports_from_use(item_use: &ItemUse) -> ImportExtraction {
    extract_imports_from_tree(&item_use.tree, "")
}

/// Check if an identifier is a relative path keyword.
fn is_relative_path_keyword(ident: &str) -> bool {
    matches!(ident, "self" | "super" | "crate")
}

/// Recursively extract imports from a use tree.
fn extract_imports_from_tree(tree: &UseTree, prefix: &str) -> ImportExtraction {
    match tree {
        UseTree::Path(path) => {
            // Check if this is a relative path (self::, super::, crate::)
            let is_relative =
                prefix.is_empty() && is_relative_path_keyword(&path.ident.to_string());

            // Build the path prefix
            let new_prefix = if prefix.is_empty() {
                path.ident.to_string()
            } else {
                format!("{prefix}::{}", path.ident)
            };
            let mut extraction = extract_imports_from_tree(&path.tree, &new_prefix);
            extraction.has_relative = extraction.has_relative || is_relative;
            extraction
        }
        UseTree::Name(name) => {
            // Final name: use foo::bar::Baz;
            let full_path = if prefix.is_empty() {
                name.ident.to_string()
            } else {
                format!("{prefix}::{}", name.ident)
            };
            ImportExtraction {
                imports: vec![ImportInfo {
                    name: name.ident.to_string(),
                    path: full_path,
                }],
                has_glob: false,
                has_relative: false,
            }
        }
        UseTree::Rename(rename) => {
            // Renamed import: use foo::bar::Baz as Qux;
            let full_path = if prefix.is_empty() {
                rename.ident.to_string()
            } else {
                format!("{prefix}::{}", rename.ident)
            };
            ImportExtraction {
                imports: vec![ImportInfo {
                    name: rename.rename.to_string(),
                    path: full_path,
                }],
                has_glob: false,
                has_relative: false,
            }
        }
        UseTree::Glob(_) => {
            // Glob import: use foo::*; - we can't resolve these
            ImportExtraction {
                imports: vec![],
                has_glob: true,
                has_relative: false,
            }
        }
        UseTree::Group(group) => {
            // Group: use foo::{Bar, Baz};
            let mut imports = Vec::new();
            let mut has_glob = false;
            let mut has_relative = false;
            for item in &group.items {
                let extraction = extract_imports_from_tree(item, prefix);
                imports.extend(extraction.imports);
                has_glob = has_glob || extraction.has_glob;
                has_relative = has_relative || extraction.has_relative;
            }
            ImportExtraction {
                imports,
                has_glob,
                has_relative,
            }
        }
    }
}

/// Extract public methods from an impl block.
fn extract_public_methods(impl_block: &ItemImpl) -> Vec<FunctionInfo> {
    let mut functions = Vec::new();

    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item {
            // Only process public methods
            if !matches!(method.vis, Visibility::Public(_)) {
                continue;
            }

            let name = method.sig.ident.clone();
            let doc = extract_doc_comment(&method.attrs);
            let is_custom = has_custom_attribute(&method.attrs);

            // Extract parameters (name and type)
            let params = extract_parameters(method);

            // Extract input type (parameters after self)
            let input_type = extract_input_type(&params);

            // Extract output type
            let output_type = extract_output_type(&method.sig.output);

            functions.push(FunctionInfo {
                name,
                doc,
                params,
                input_type,
                output_type,
                is_custom,
            });
        }
    }

    functions
}

/// Extract parameter names and types from a method (excluding self).
fn extract_parameters(method: &ImplItemFn) -> Vec<ParameterInfo> {
    method
        .sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pat_type) = arg {
                // Extract parameter name from pattern
                let name = if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    pat_ident.ident.clone()
                } else {
                    // Fallback for complex patterns
                    format_ident!("arg")
                };
                let ty = {
                    let t = &pat_type.ty;
                    quote! { #t }
                };
                Some(ParameterInfo { name, ty })
            } else {
                None // Skip self parameters
            }
        })
        .collect()
}

/// Extract doc comments from attributes.
fn extract_doc_comment(attrs: &[Attribute]) -> Option<String> {
    let docs: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc")
                && let syn::Meta::NameValue(meta) = &attr.meta
                && let Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) = &meta.value
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

/// Build the input type from extracted parameters.
fn extract_input_type(params: &[ParameterInfo]) -> TokenStream2 {
    match params.len() {
        0 => quote! { () },
        1 => {
            let ty = &params[0].ty;
            quote! { #ty }
        }
        _ => {
            // Multiple parameters - create a tuple type
            let types: Vec<_> = params.iter().map(|p| &p.ty).collect();
            quote! { (#(#types),*) }
        }
    }
}

/// Extract the output type from a return type.
fn extract_output_type(ret: &ReturnType) -> TokenStream2 {
    match ret {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => quote! { #ty },
    }
}

/// Extract all `abi::emit()` calls from an impl block.
fn extract_emit_calls(impl_block: &ItemImpl) -> Vec<EventInfo> {
    let mut visitor = EmitVisitor::new();
    visitor.visit_item_impl(impl_block);

    // Deduplicate events by topic (keep first occurrence)
    let mut seen = std::collections::HashSet::new();
    visitor.events.into_iter().filter(|e| seen.insert(e.topic.clone())).collect()
}

/// Generate the schema constant.
fn generate_schema(
    contract_name: &str,
    imports: &[ImportInfo],
    functions: &[FunctionInfo],
    events: &[EventInfo],
) -> TokenStream2 {
    let contract_name_lit = contract_name;

    let import_entries: Vec<_> = imports
        .iter()
        .map(|i| {
            let name = &i.name;
            let path = &i.path;

            quote! {
                dusk_wasm::schema::ImportSchema {
                    name: #name,
                    path: #path,
                }
            }
        })
        .collect();

    let function_entries: Vec<_> = functions
        .iter()
        .map(|f| {
            let name_str = f.name.to_string();
            let doc = f.doc.as_deref().unwrap_or("");
            let input = &f.input_type;
            let output = &f.output_type;
            let custom = f.is_custom;

            // Convert type tokens to string for the schema
            let input_str = input.to_string();
            let output_str = output.to_string();

            quote! {
                dusk_wasm::schema::FunctionSchema {
                    name: #name_str,
                    doc: #doc,
                    input: #input_str,
                    output: #output_str,
                    custom: #custom,
                }
            }
        })
        .collect();

    let event_entries: Vec<_> = events
        .iter()
        .map(|e| {
            let topic = &e.topic;
            let data = &e.data_type;

            // Convert type tokens to string for the schema
            let data_str = data.to_string();

            quote! {
                dusk_wasm::schema::EventSchema {
                    topic: #topic,
                    data: #data_str,
                }
            }
        })
        .collect();

    quote! {
        /// Contract schema containing metadata about functions, events, and imports.
        pub const CONTRACT_SCHEMA: dusk_wasm::schema::ContractSchema = dusk_wasm::schema::ContractSchema {
            name: #contract_name_lit,
            imports: &[#(#import_entries),*],
            functions: &[#(#function_entries),*],
            events: &[#(#event_entries),*],
        };
    }
}

/// Generate extern "C" wrapper functions for all public methods.
///
/// Each wrapper deserializes input, calls the method on STATE, and serializes output.
fn generate_extern_wrappers(functions: &[FunctionInfo]) -> TokenStream2 {
    let wrappers: Vec<_> = functions
        .iter()
        .map(|f| {
            let fn_name = &f.name;
            let input_type = &f.input_type;

            // Build the closure parameter pattern and the method call arguments
            let (closure_param, method_args) = match f.params.len() {
                0 => {
                    // No parameters: |(): ()|
                    (quote! { (): () }, quote! {})
                }
                1 => {
                    // Single parameter: |name: Type|
                    let param = &f.params[0];
                    let name = &param.name;
                    let ty = &param.ty;
                    (quote! { #name: #ty }, quote! { #name })
                }
                _ => {
                    // Multiple parameters: |(p1, p2, ...): (T1, T2, ...)|
                    let names: Vec<_> = f.params.iter().map(|p| &p.name).collect();
                    (
                        quote! { (#(#names),*): #input_type },
                        quote! { #(#names),* },
                    )
                }
            };

            quote! {
                #[no_mangle]
                unsafe extern "C" fn #fn_name(arg_len: u32) -> u32 {
                    dusk_core::abi::wrap_call(arg_len, |#closure_param| STATE.#fn_name(#method_args))
                }
            }
        })
        .collect();

    quote! {
        #[cfg(target_family = "wasm")]
        mod __contract_extern_wrappers {
            use super::*;

            #(#wrappers)*
        }
    }
}

/// Strip #[contract(...)] attributes from methods in the impl block.
fn strip_contract_attributes(mut impl_block: ItemImpl) -> ItemImpl {
    for item in &mut impl_block.items {
        if let ImplItem::Fn(method) = item {
            method.attrs.retain(|attr| !attr.path().is_ident("contract"));
        }
    }
    impl_block
}

/// Validated contract module data extracted during parsing.
struct ContractData<'a> {
    imports: Vec<ImportInfo>,
    contract_name: String,
    impl_blocks: Vec<&'a ItemImpl>,
}

/// Validate that a public method has a supported signature for extern wrapper generation.
///
/// Returns an error if the method:
/// - Has no `self` receiver (associated function)
/// - Has generic type parameters
/// - Is async
/// - Consumes `self` (not `&self` or `&mut self`)
fn validate_public_method(method: &ImplItemFn) -> Result<(), syn::Error> {
    let name = &method.sig.ident;

    // Check for generic type parameters
    if !method.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &method.sig.generics,
            format!(
                "public method `{name}` cannot have generic parameters; \
                 extern \"C\" wrappers require concrete types"
            ),
        ));
    }

    // Check for async
    if method.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            method.sig.asyncness,
            format!(
                "public method `{name}` cannot be async; \
                 WASM contracts do not support async execution"
            ),
        ));
    }

    // Check for self receiver
    let receiver = method.sig.inputs.first().and_then(|arg| {
        if let FnArg::Receiver(r) = arg {
            Some(r)
        } else {
            None
        }
    });

    let Some(receiver) = receiver else {
        return Err(syn::Error::new_spanned(
            &method.sig,
            format!(
                "public method `{name}` must have a `self` receiver; \
                 associated functions cannot be exposed as contract methods"
            ),
        ));
    };

    // Check that self is borrowed, not consumed
    if receiver.reference.is_none() {
        return Err(syn::Error::new_spanned(
            receiver,
            format!(
                "public method `{name}` cannot consume `self`; \
                 use `&self` or `&mut self` instead"
            ),
        ));
    }

    Ok(())
}

/// Validate all public methods in an impl block.
fn validate_impl_block_methods(impl_block: &ItemImpl) -> Result<(), syn::Error> {
    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item
            && matches!(method.vis, Visibility::Public(_))
        {
            validate_public_method(method)?;
        }
    }
    Ok(())
}

/// Validate the module and extract contract data.
///
/// Returns an error if validation fails.
fn validate_and_extract<'a>(
    module: &'a ItemMod,
    items: &'a [Item],
) -> Result<ContractData<'a>, syn::Error> {
    // Extract all use statements and build import map, checking for unsupported imports
    let mut imports = Vec::new();
    let mut glob_imports = Vec::new();
    let mut relative_imports = Vec::new();

    for item in items {
        if let Item::Use(item_use) = item {
            let extraction = extract_imports_from_use(item_use);
            imports.extend(extraction.imports);
            if extraction.has_glob {
                glob_imports.push(item_use);
            }
            if extraction.has_relative {
                relative_imports.push(item_use);
            }
        }
    }

    // Error on glob imports - we can't track their paths
    if let Some(first_glob) = glob_imports.first() {
        return Err(syn::Error::new_spanned(
            first_glob,
            "#[contract] does not support glob imports (`use foo::*`); \
             import types explicitly so their paths can be tracked",
        ));
    }

    // Error on relative imports - we need absolute paths for code generation
    if let Some(first_relative) = relative_imports.first() {
        return Err(syn::Error::new_spanned(
            first_relative,
            "#[contract] does not support relative imports (`use self::`, `use super::`, `use crate::`); \
             use absolute paths so they can be resolved for code generation",
        ));
    }

    // Find all pub structs and ensure there's exactly one
    let pub_structs: Vec<_> = items
        .iter()
        .filter_map(|item| {
            if let Item::Struct(s) = item
                && matches!(s.vis, Visibility::Public(_))
            {
                Some(s)
            } else {
                None
            }
        })
        .collect();

    if pub_structs.is_empty() {
        return Err(syn::Error::new_spanned(
            module,
            "#[contract] module must contain a pub struct for the contract state",
        ));
    }

    if pub_structs.len() > 1 {
        return Err(syn::Error::new_spanned(
            pub_structs[1],
            "#[contract] module must contain exactly one pub struct; \
             found multiple public structs",
        ));
    }

    let contract_struct = pub_structs[0];
    let contract_name = contract_struct.ident.to_string();

    // Find impl blocks for the contract struct
    let impl_blocks: Vec<&ItemImpl> = items
        .iter()
        .filter_map(|item| {
            if let Item::Impl(impl_block) = item
                && impl_block.trait_.is_none()
                && let Type::Path(type_path) = &*impl_block.self_ty
                && type_path.path.is_ident(&contract_name)
            {
                Some(impl_block)
            } else {
                None
            }
        })
        .collect();

    // Ensure there's at least one impl block
    if impl_blocks.is_empty() {
        return Err(syn::Error::new_spanned(
            contract_struct,
            format!("#[contract] module must contain an impl block for `{contract_name}`"),
        ));
    }

    // Validate all public methods in impl blocks
    for impl_block in &impl_blocks {
        validate_impl_block_methods(impl_block)?;
    }

    Ok(ContractData {
        imports,
        contract_name,
        impl_blocks,
    })
}

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
/// - A public method has generic type parameters
/// - A public method is async
/// - A public method consumes `self` instead of borrowing it
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
    let data = match validate_and_extract(&module, items) {
        Ok(data) => data,
        Err(e) => return e.to_compile_error().into(),
    };

    let ContractData {
        imports,
        contract_name,
        impl_blocks,
    } = data;

    // Extract functions and events from all impl blocks
    let mut functions = Vec::new();
    let mut events = Vec::new();

    for impl_block in &impl_blocks {
        functions.extend(extract_public_methods(impl_block));
        events.extend(extract_emit_calls(impl_block));
    }

    // Deduplicate events by topic
    let mut seen = std::collections::HashSet::new();
    let events: Vec<_> = events.into_iter().filter(|e| seen.insert(e.topic.clone())).collect();

    // Generate schema
    let schema = generate_schema(&contract_name, &imports, &functions, &events);

    // Generate extern "C" wrappers
    let externs = generate_extern_wrappers(&functions);

    // Rebuild the module with stripped contract attributes on methods
    let mod_vis = &module.vis;
    let mod_name = &module.ident;
    let mod_attrs = &module.attrs;

    let new_items: Vec<_> = items
        .iter()
        .map(|item| {
            if let Item::Impl(impl_block) = item
                && impl_block.trait_.is_none()
                && let Type::Path(type_path) = &*impl_block.self_ty
                && type_path.path.is_ident(&contract_name)
            {
                Item::Impl(strip_contract_attributes(impl_block.clone()))
            } else {
                item.clone()
            }
        })
        .collect();

    // Output: module with schema and externs added
    let output = quote! {
        #(#mod_attrs)*
        #mod_vis mod #mod_name {
            #(#new_items)*

            #schema

            #externs
        }
    };

    output.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::format_ident;

    fn normalize_tokens(tokens: TokenStream2) -> String {
        // Normalize whitespace for comparison
        tokens
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn test_extract_imports_simple() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use evm_core::standard_bridge::SetU64;
        };
        let extraction = extract_imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 1);
        assert_eq!(extraction.imports[0].name, "SetU64");
        assert_eq!(extraction.imports[0].path, "evm_core::standard_bridge::SetU64");
        assert!(!extraction.has_glob);
        assert!(!extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_renamed() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use dusk_core::Address as DSAddress;
        };
        let extraction = extract_imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 1);
        assert_eq!(extraction.imports[0].name, "DSAddress");
        assert_eq!(extraction.imports[0].path, "dusk_core::Address");
        assert!(!extraction.has_glob);
        assert!(!extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_group() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use evm_core::standard_bridge::{SetU64, Deposit, EVMAddress};
        };
        let extraction = extract_imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 3);
        assert!(!extraction.has_glob);
        assert!(!extraction.has_relative);

        let names: Vec<_> = extraction.imports.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"SetU64"));
        assert!(names.contains(&"Deposit"));
        assert!(names.contains(&"EVMAddress"));

        let set_u64 = extraction.imports.iter().find(|i| i.name == "SetU64").unwrap();
        assert_eq!(set_u64.path, "evm_core::standard_bridge::SetU64");
    }

    #[test]
    fn test_extract_imports_glob() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use evm_core::standard_bridge::*;
        };
        let extraction = extract_imports_from_use(&use_stmt);
        assert!(extraction.imports.is_empty());
        assert!(extraction.has_glob);
        assert!(!extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_group_with_glob() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use evm_core::standard_bridge::{SetU64, events::*};
        };
        let extraction = extract_imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 1);
        assert_eq!(extraction.imports[0].name, "SetU64");
        assert!(extraction.has_glob);
        assert!(!extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_relative_self() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use self::types::MyType;
        };
        let extraction = extract_imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 1);
        assert_eq!(extraction.imports[0].name, "MyType");
        assert_eq!(extraction.imports[0].path, "self::types::MyType");
        assert!(!extraction.has_glob);
        assert!(extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_relative_super() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use super::common::SharedType;
        };
        let extraction = extract_imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 1);
        assert_eq!(extraction.imports[0].name, "SharedType");
        assert_eq!(extraction.imports[0].path, "super::common::SharedType");
        assert!(!extraction.has_glob);
        assert!(extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_relative_crate() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use crate::utils::Helper;
        };
        let extraction = extract_imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 1);
        assert_eq!(extraction.imports[0].name, "Helper");
        assert_eq!(extraction.imports[0].path, "crate::utils::Helper");
        assert!(!extraction.has_glob);
        assert!(extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_group_with_relative() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use self::types::{TypeA, TypeB};
        };
        let extraction = extract_imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 2);
        assert!(!extraction.has_glob);
        assert!(extraction.has_relative);
    }

    #[test]
    fn test_extern_wrapper_no_params() {
        let functions = vec![FunctionInfo {
            name: format_ident!("is_paused"),
            doc: Some("Returns pause state.".to_string()),
            params: vec![],
            input_type: quote! { () },
            output_type: quote! { bool },
            is_custom: false,
        }];

        let output = normalize_tokens(generate_extern_wrappers(&functions));

        let expected = normalize_tokens(quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[no_mangle]
                unsafe extern "C" fn is_paused(arg_len: u32) -> u32 {
                    dusk_core::abi::wrap_call(arg_len, |(): ()| STATE.is_paused())
                }
            }
        });

        assert_eq!(expected, output);
    }

    #[test]
    fn test_extern_wrapper_single_param() {
        let functions = vec![FunctionInfo {
            name: format_ident!("init"),
            doc: Some("Initialize.".to_string()),
            params: vec![ParameterInfo {
                name: format_ident!("owner"),
                ty: quote! { Address },
            }],
            input_type: quote! { Address },
            output_type: quote! { () },
            is_custom: false,
        }];

        let output = normalize_tokens(generate_extern_wrappers(&functions));

        let expected = normalize_tokens(quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[no_mangle]
                unsafe extern "C" fn init(arg_len: u32) -> u32 {
                    dusk_core::abi::wrap_call(arg_len, |owner: Address| STATE.init(owner))
                }
            }
        });

        assert_eq!(expected, output);
    }

    #[test]
    fn test_extern_wrapper_multiple_params() {
        let functions = vec![FunctionInfo {
            name: format_ident!("transfer"),
            doc: Some("Transfer funds.".to_string()),
            params: vec![
                ParameterInfo {
                    name: format_ident!("to"),
                    ty: quote! { Address },
                },
                ParameterInfo {
                    name: format_ident!("amount"),
                    ty: quote! { u64 },
                },
            ],
            input_type: quote! { (Address, u64) },
            output_type: quote! { () },
            is_custom: false,
        }];

        let output = normalize_tokens(generate_extern_wrappers(&functions));

        let expected = normalize_tokens(quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[no_mangle]
                unsafe extern "C" fn transfer(arg_len: u32) -> u32 {
                    dusk_core::abi::wrap_call(arg_len, |(to, amount): (Address, u64)| STATE.transfer(to, amount))
                }
            }
        });

        assert_eq!(expected, output);
    }

    #[test]
    fn test_extern_wrappers_multiple_functions() {
        let functions = vec![
            FunctionInfo {
                name: format_ident!("pause"),
                doc: None,
                params: vec![],
                input_type: quote! { () },
                output_type: quote! { () },
                is_custom: false,
            },
            FunctionInfo {
                name: format_ident!("unpause"),
                doc: None,
                params: vec![],
                input_type: quote! { () },
                output_type: quote! { () },
                is_custom: false,
            },
        ];

        let output = normalize_tokens(generate_extern_wrappers(&functions));

        let expected = normalize_tokens(quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[no_mangle]
                unsafe extern "C" fn pause(arg_len: u32) -> u32 {
                    dusk_core::abi::wrap_call(arg_len, |(): ()| STATE.pause())
                }

                #[no_mangle]
                unsafe extern "C" fn unpause(arg_len: u32) -> u32 {
                    dusk_core::abi::wrap_call(arg_len, |(): ()| STATE.unpause())
                }
            }
        });

        assert_eq!(expected, output);
    }

    #[test]
    fn test_validate_method_valid_ref_self() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn get_value(&self) -> u64 { 0 }
        };
        assert!(validate_public_method(&method).is_ok());
    }

    #[test]
    fn test_validate_method_valid_mut_self() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn set_value(&mut self, value: u64) { }
        };
        assert!(validate_public_method(&method).is_ok());
    }

    #[test]
    fn test_validate_method_no_self() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn new() -> Self { Self }
        };
        let err = validate_public_method(&method).unwrap_err();
        assert!(err.to_string().contains("must have a `self` receiver"));
    }

    #[test]
    fn test_validate_method_consuming_self() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn destroy(self) { }
        };
        let err = validate_public_method(&method).unwrap_err();
        assert!(err.to_string().contains("cannot consume `self`"));
    }

    #[test]
    fn test_validate_method_generic() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn process<T>(&self, value: T) -> T { value }
        };
        let err = validate_public_method(&method).unwrap_err();
        assert!(err.to_string().contains("cannot have generic parameters"));
    }

    #[test]
    fn test_validate_method_async() {
        let method: ImplItemFn = syn::parse_quote! {
            pub async fn fetch_data(&self) -> u64 { 0 }
        };
        let err = validate_public_method(&method).unwrap_err();
        assert!(err.to_string().contains("cannot be async"));
    }
}
