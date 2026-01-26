// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Extraction functions for contract metadata.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, Expr, ExprLit, FnArg, ImplItem, ImplItemFn, Item, ItemImpl, ItemMod, Lit, Pat,
    ReturnType, Type, Visibility,
};

use crate::{
    extract_doc_comment, extract_feeds_attribute, extract_receiver, has_custom_attribute,
    has_empty_body, has_feed_calls, parse, validate, ContractData, CustomDataDriverHandler,
    DataDriverRole, EmitVisitor, EventInfo, FunctionInfo, ImportInfo, ParameterInfo, TraitImplInfo,
};

/// Extract topic string from the first argument of `abi::emit()`.
/// Handles both string literals and const path expressions.
pub(crate) fn topic_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        // String literal: "topic_name"
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => Some(s.value()),
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
pub(crate) fn type_from_expr(expr: &Expr) -> TokenStream2 {
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

/// Extract methods from a trait impl block based on the expose list.
///
/// Only methods whose names appear in the `expose_list` will be extracted.
/// Methods with empty bodies `{}` are treated as "use default implementation" -
/// the macro will generate wrappers that call the trait method directly.
pub(crate) fn trait_methods(trait_impl: &TraitImplInfo) -> Result<Vec<FunctionInfo>, syn::Error> {
    let mut functions = Vec::new();

    for item in &trait_impl.impl_block.items {
        if let ImplItem::Fn(method) = item {
            let method_name = method.sig.ident.to_string();

            // Only process methods in the expose list
            if !trait_impl.expose_list.contains(&method_name) {
                continue;
            }

            // Check if this is an empty-body method (signals "use default impl")
            let is_default_impl = has_empty_body(method);

            // Validate the method (allow associated functions for trait methods)
            validate::trait_method(method, &trait_impl.trait_name, is_default_impl)?;

            let name = method.sig.ident.clone();
            let doc = extract_doc_comment(&method.attrs);
            let is_custom = has_custom_attribute(&method.attrs);
            let feed_type = extract_feeds_attribute(&method.attrs);
            let receiver = extract_receiver(method);

            // Validate: if method uses abi::feed(), it must have #[contract(feeds = "Type")]
            // (only check non-empty bodies since empty bodies delegate to trait defaults)
            if !is_default_impl && has_feed_calls(method) && feed_type.is_none() {
                return Err(syn::Error::new_spanned(
                    &method.sig,
                    format!(
                        "method `{name}` uses `abi::feed()` but is missing `#[contract(feeds = \"Type\")]` attribute; \
                         add the attribute to specify the type being fed for data-driver decoding"
                    ),
                ));
            }

            // Extract parameters (name and type)
            let params = parameters(method);

            // Extract input type (parameters after self)
            let input_type = input_type(&params);

            // Extract output type (dereferenced if it's a reference)
            let (output_type, returns_ref) = output_type(&method.sig.output);

            // For empty-body methods, store the trait name to generate correct wrapper
            let trait_name = if is_default_impl {
                Some(trait_impl.trait_name.clone())
            } else {
                None
            };

            functions.push(FunctionInfo {
                name,
                doc,
                params,
                input_type,
                output_type,
                is_custom,
                returns_ref,
                receiver,
                trait_name,
                feed_type,
            });
        }
    }

    // Check that all methods in expose list were found
    for method_name in &trait_impl.expose_list {
        if !functions.iter().any(|f| f.name == method_name) {
            return Err(syn::Error::new_spanned(
                trait_impl.impl_block,
                format!(
                    "method `{method_name}` listed in expose but not found in `impl {} for ...`; \
                     add a stub with empty body `{{}}` to expose default implementations",
                    trait_impl.trait_name
                ),
            ));
        }
    }

    Ok(functions)
}

/// Extract public methods from an impl block.
///
/// Note: The `new` method is skipped because it's a special constructor
/// used only for initializing the static STATE variable.
///
/// Returns an error if a method uses `abi::feed()` but lacks the
/// `#[contract(feeds = "Type")]` attribute.
pub(crate) fn public_methods(impl_block: &ItemImpl) -> Result<Vec<FunctionInfo>, syn::Error> {
    let mut functions = Vec::new();

    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item {
            // Only process public methods
            if !matches!(method.vis, Visibility::Public(_)) {
                continue;
            }

            // Skip the `new` constructor - it's not exported
            if method.sig.ident == "new" {
                continue;
            }

            let name = method.sig.ident.clone();
            let doc = extract_doc_comment(&method.attrs);
            let is_custom = has_custom_attribute(&method.attrs);
            let feed_type = extract_feeds_attribute(&method.attrs);
            let receiver = extract_receiver(method);

            // Validate: if method uses abi::feed(), it must have #[contract(feeds = "Type")]
            if has_feed_calls(method) && feed_type.is_none() {
                return Err(syn::Error::new_spanned(
                    &method.sig,
                    format!(
                        "method `{name}` uses `abi::feed()` but is missing `#[contract(feeds = \"Type\")]` attribute; \
                         add the attribute to specify the type being fed for data-driver decoding"
                    ),
                ));
            }

            // Extract parameters (name and type)
            let params = parameters(method);

            // Extract input type (parameters after self)
            let input_type = input_type(&params);

            // Extract output type (dereferenced if it's a reference)
            let (output_type, returns_ref) = output_type(&method.sig.output);

            functions.push(FunctionInfo {
                name,
                doc,
                params,
                input_type,
                output_type,
                is_custom,
                returns_ref,
                receiver,
                trait_name: None, // Not a trait method
                feed_type,
            });
        }
    }

    Ok(functions)
}

/// Extract parameter names and types from a method (excluding self).
///
/// For reference parameters (`&T` or `&mut T`), extracts the inner type
/// and marks them accordingly for wrapper generation.
pub(crate) fn parameters(method: &ImplItemFn) -> Vec<ParameterInfo> {
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

                // Check if the type is a reference and extract inner type
                let (ty, is_ref, is_mut_ref) = if let Type::Reference(type_ref) = &*pat_type.ty {
                    let inner = &type_ref.elem;
                    let is_mut = type_ref.mutability.is_some();
                    (quote! { #inner }, true, is_mut)
                } else {
                    let t = &pat_type.ty;
                    (quote! { #t }, false, false)
                };

                Some(ParameterInfo {
                    name,
                    ty,
                    is_ref,
                    is_mut_ref,
                })
            } else {
                None // Skip self parameters
            }
        })
        .collect()
}

/// Build the input type from extracted parameters.
pub(crate) fn input_type(params: &[ParameterInfo]) -> TokenStream2 {
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
///
/// If the return type is a reference (`&T` or `&mut T`), returns the inner type
/// and `true`. Otherwise returns the type as-is and `false`.
pub(crate) fn output_type(ret: &ReturnType) -> (TokenStream2, bool) {
    match ret {
        ReturnType::Default => (quote! { () }, false),
        ReturnType::Type(_, ty) => {
            // Check if it's a reference type
            if let Type::Reference(type_ref) = &**ty {
                let inner = &type_ref.elem;
                (quote! { #inner }, true)
            } else {
                (quote! { #ty }, false)
            }
        }
    }
}

/// Extract all `abi::emit()` calls from an impl block.
///
/// Events are deduplicated by topic, keeping only the first occurrence.
pub(crate) fn emit_calls(impl_block: &ItemImpl) -> Vec<EventInfo> {
    use syn::visit::Visit;

    let mut visitor = EmitVisitor::new();
    visitor.visit_item_impl(impl_block);

    // Deduplicate events by topic (keep first occurrence)
    let mut seen = std::collections::HashSet::new();
    visitor
        .events
        .into_iter()
        .filter(|e| seen.insert(e.topic.clone()))
        .collect()
}

/// Extract the `expose = [method1, method2, ...]` list from a `#[contract(...)]` attribute.
///
/// Returns `None` if there's no `#[contract(expose = [...])]` attribute.
/// Returns `Some(vec![...])` with the method names if found.
pub(crate) fn expose_list(attrs: &[Attribute]) -> Option<Vec<String>> {
    for attr in attrs {
        if !attr.path().is_ident("contract") {
            continue;
        }

        let Ok(meta) = attr.meta.require_list() else {
            continue;
        };

        // Parse: expose = [method1, method2, ...]
        let tokens = meta.tokens.clone();
        let mut iter = tokens.into_iter().peekable();

        // Look for "expose"
        let Some(proc_macro2::TokenTree::Ident(ident)) = iter.next() else {
            continue;
        };
        if ident != "expose" {
            continue;
        }

        // Expect "="
        let Some(proc_macro2::TokenTree::Punct(punct)) = iter.next() else {
            continue;
        };
        if punct.as_char() != '=' {
            continue;
        }

        // Expect "[...]"
        let Some(proc_macro2::TokenTree::Group(group)) = iter.next() else {
            continue;
        };
        if group.delimiter() != proc_macro2::Delimiter::Bracket {
            continue;
        }

        // Parse the method names from the group
        let mut methods = Vec::new();
        for token in group.stream() {
            if let proc_macro2::TokenTree::Ident(method_ident) = token {
                methods.push(method_ident.to_string());
            }
            // Skip commas and other punctuation
        }

        return Some(methods);
    }

    None
}

// ============================================================================
// Contract Data Extraction
// ============================================================================

/// Extract and validate imports from the module items.
///
/// Returns an error if glob or relative imports are found.
fn imports(items: &[Item]) -> Result<Vec<ImportInfo>, syn::Error> {
    let mut result = Vec::new();
    let mut glob_import = None;
    let mut relative_import = None;

    for item in items {
        if let Item::Use(item_use) = item {
            let extraction = parse::imports_from_use(item_use);
            result.extend(extraction.imports);
            if extraction.has_glob && glob_import.is_none() {
                glob_import = Some(item_use);
            }
            if extraction.has_relative && relative_import.is_none() {
                relative_import = Some(item_use);
            }
        }
    }

    if let Some(item_use) = glob_import {
        return Err(syn::Error::new_spanned(
            item_use,
            "#[contract] does not support glob imports (`use foo::*`); \
             import types explicitly so their paths can be tracked",
        ));
    }

    if let Some(item_use) = relative_import {
        return Err(syn::Error::new_spanned(
            item_use,
            "#[contract] does not support relative imports (`use self::`, `use super::`, `use crate::`); \
             use absolute paths so they can be resolved for code generation",
        ));
    }

    Ok(result)
}

/// Find the contract struct in the module.
///
/// The module must contain exactly one `pub struct` which serves as the contract state.
/// Returns an error if there are zero or multiple public structs.
fn contract_struct<'a>(
    module: &'a ItemMod,
    items: &'a [Item],
) -> Result<&'a syn::ItemStruct, syn::Error> {
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

    Ok(pub_structs[0])
}

/// Find inherent impl blocks for the contract struct.
///
/// Returns all `impl ContractName { ... }` blocks (without a trait).
fn impl_blocks<'a>(items: &'a [Item], contract_name: &str) -> Vec<&'a ItemImpl> {
    items
        .iter()
        .filter_map(|item| {
            if let Item::Impl(impl_block) = item
                && impl_block.trait_.is_none()
                && let Type::Path(type_path) = &*impl_block.self_ty
                && type_path.path.is_ident(contract_name)
            {
                Some(impl_block)
            } else {
                None
            }
        })
        .collect()
}

/// Find trait impl blocks with `#[contract(expose = [...])]` attributes.
///
/// Only trait implementations that have an explicit expose list are returned.
/// The expose list specifies which trait methods should have extern wrappers generated.
fn trait_impls<'a>(items: &'a [Item], contract_name: &str) -> Vec<TraitImplInfo<'a>> {
    items
        .iter()
        .filter_map(|item| {
            if let Item::Impl(impl_block) = item
                && let Some((_, trait_path, _)) = &impl_block.trait_
                && let Type::Path(type_path) = &*impl_block.self_ty
                && type_path.path.is_ident(contract_name)
                && let Some(list) = expose_list(&impl_block.attrs)
            {
                let trait_name = trait_path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                Some(TraitImplInfo {
                    trait_name,
                    impl_block,
                    expose_list: list,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Extract custom data-driver handler functions from module items.
///
/// Looks for functions with attributes like:
/// - `#[contract(encode_input = "fn_name")]`
/// - `#[contract(decode_input = "fn_name")]`
/// - `#[contract(decode_output = "fn_name")]`
fn custom_data_driver_handlers(items: &[Item]) -> Vec<CustomDataDriverHandler> {
    let mut handlers = Vec::new();

    for item in items {
        let Item::Fn(func) = item else {
            continue;
        };

        for attr in &func.attrs {
            if !attr.path().is_ident("contract") {
                continue;
            }

            let Ok(meta) = attr.meta.require_list() else {
                continue;
            };

            // Parse: encode_input = "fn_name", decode_input = "fn_name", or decode_output = "fn_name"
            let tokens = meta.tokens.clone();
            let mut iter = tokens.into_iter().peekable();

            // Look for role identifier (encode_input, decode_input, decode_output)
            let Some(proc_macro2::TokenTree::Ident(role_ident)) = iter.next() else {
                continue;
            };

            let role = match role_ident.to_string().as_str() {
                "encode_input" => DataDriverRole::EncodeInput,
                "decode_input" => DataDriverRole::DecodeInput,
                "decode_output" => DataDriverRole::DecodeOutput,
                _ => continue,
            };

            // Expect "="
            let Some(proc_macro2::TokenTree::Punct(punct)) = iter.next() else {
                continue;
            };
            if punct.as_char() != '=' {
                continue;
            }

            // Expect string literal with function name
            let Some(proc_macro2::TokenTree::Literal(lit)) = iter.next() else {
                continue;
            };
            let lit_str = lit.to_string();
            // Remove quotes from the literal
            let fn_name = lit_str.trim_matches('"').to_string();

            // Clone the function without the contract attribute
            let mut func_clone = func.clone();
            func_clone.attrs.retain(|a| !a.path().is_ident("contract"));

            handlers.push(CustomDataDriverHandler {
                fn_name,
                role,
                func: func_clone,
            });
        }
    }

    handlers
}

/// Check if an item is a custom data-driver handler function.
///
/// Returns true if the item has a `#[contract(encode_input = ...)]`,
/// `#[contract(decode_input = ...)]`, or `#[contract(decode_output = ...)]` attribute.
pub(crate) fn is_custom_handler(item: &Item) -> bool {
    let Item::Fn(func) = item else {
        return false;
    };

    for attr in &func.attrs {
        if !attr.path().is_ident("contract") {
            continue;
        }

        let Ok(meta) = attr.meta.require_list() else {
            continue;
        };

        let tokens = meta.tokens.clone();
        let mut iter = tokens.into_iter();

        if let Some(proc_macro2::TokenTree::Ident(ident)) = iter.next() {
            let name = ident.to_string();
            if name == "encode_input" || name == "decode_input" || name == "decode_output" {
                return true;
            }
        }
    }

    false
}

/// Extract contract data from the module, validating constraints.
///
/// Returns an error if validation fails.
pub(crate) fn contract_data<'a>(
    module: &'a ItemMod,
    items: &'a [Item],
) -> Result<ContractData<'a>, syn::Error> {
    let imports = imports(items)?;
    let struct_ = contract_struct(module, items)?;
    let name = struct_.ident.to_string();

    let impl_blocks = impl_blocks(items, &name);
    if impl_blocks.is_empty() {
        return Err(syn::Error::new_spanned(
            struct_,
            format!("#[contract] module must contain an impl block for `{name}`"),
        ));
    }

    for impl_block in &impl_blocks {
        validate::impl_block_methods(impl_block)?;
    }

    validate::new_constructor(&name, &impl_blocks, struct_)?;
    validate::init_method(&name, &impl_blocks)?;

    let trait_impls = trait_impls(items, &name);
    let custom_handlers = custom_data_driver_handlers(items);

    Ok(ContractData {
        imports,
        contract_name: name,
        contract_ident: struct_.ident.clone(),
        impl_blocks,
        trait_impls,
        custom_handlers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalize_tokens(tokens: TokenStream2) -> String {
        tokens
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn test_output_type_value() {
        let ret: ReturnType = syn::parse_quote! { -> u64 };
        let (ty, returns_ref) = output_type(&ret);
        assert_eq!(normalize_tokens(ty), "u64");
        assert!(!returns_ref);
    }

    #[test]
    fn test_output_type_ref() {
        let ret: ReturnType = syn::parse_quote! { -> &LargeStruct };
        let (ty, returns_ref) = output_type(&ret);
        assert_eq!(normalize_tokens(ty), "LargeStruct");
        assert!(returns_ref);
    }

    #[test]
    fn test_output_type_mut_ref() {
        let ret: ReturnType = syn::parse_quote! { -> &mut Data };
        let (ty, returns_ref) = output_type(&ret);
        assert_eq!(normalize_tokens(ty), "Data");
        assert!(returns_ref);
    }

    #[test]
    fn test_parameters_ref() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn process(&self, data: &LargeStruct) {}
        };
        let params = parameters(&method);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name.to_string(), "data");
        assert_eq!(normalize_tokens(params[0].ty.clone()), "LargeStruct");
        assert!(params[0].is_ref);
        assert!(!params[0].is_mut_ref);
    }

    #[test]
    fn test_parameters_mut_ref() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn modify(&mut self, data: &mut Data) {}
        };
        let params = parameters(&method);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name.to_string(), "data");
        assert_eq!(normalize_tokens(params[0].ty.clone()), "Data");
        assert!(params[0].is_ref);
        assert!(params[0].is_mut_ref);
    }

    #[test]
    fn test_expose_list_simple() {
        let impl_block: ItemImpl = syn::parse_quote! {
            #[contract(expose = [owner, transfer_ownership])]
            impl OwnableTrait for MyContract {
                fn owner(&self) -> Address { self.owner }
            }
        };
        let expose_list = expose_list(&impl_block.attrs);
        assert!(expose_list.is_some());
        let list = expose_list.unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"owner".to_string()));
        assert!(list.contains(&"transfer_ownership".to_string()));
    }

    #[test]
    fn test_expose_list_single() {
        let impl_block: ItemImpl = syn::parse_quote! {
            #[contract(expose = [version])]
            impl ISemver for MyContract {}
        };
        let expose_list = expose_list(&impl_block.attrs);
        assert!(expose_list.is_some());
        let list = expose_list.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], "version");
    }

    #[test]
    fn test_expose_list_none() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl OwnableTrait for MyContract {
                fn owner(&self) -> Address { self.owner }
            }
        };
        let expose_list = expose_list(&impl_block.attrs);
        assert!(expose_list.is_none());
    }

    #[test]
    fn test_expose_list_other_attribute() {
        let impl_block: ItemImpl = syn::parse_quote! {
            #[derive(Debug)]
            impl OwnableTrait for MyContract {
                fn owner(&self) -> Address { self.owner }
            }
        };
        let expose_list = expose_list(&impl_block.attrs);
        assert!(expose_list.is_none());
    }

    #[test]
    fn test_trait_methods_success() {
        let impl_block: ItemImpl = syn::parse_quote! {
            #[contract(expose = [owner])]
            impl OwnableTrait for MyContract {
                fn owner(&self) -> Option<Address> { self.owner }
                fn owner_mut(&mut self) -> &mut Option<Address> { &mut self.owner }
            }
        };
        let trait_impl = TraitImplInfo {
            trait_name: "OwnableTrait".to_string(),
            impl_block: &impl_block,
            expose_list: vec!["owner".to_string()],
        };
        let result = trait_methods(&trait_impl);
        assert!(result.is_ok());
        let functions = result.unwrap();
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name.to_string(), "owner");
    }

    #[test]
    fn test_trait_methods_multiple() {
        let impl_block: ItemImpl = syn::parse_quote! {
            #[contract(expose = [owner, transfer_ownership])]
            impl OwnableTrait for MyContract {
                fn owner(&self) -> Option<Address> { self.owner }
                fn owner_mut(&mut self) -> &mut Option<Address> { &mut self.owner }
                fn transfer_ownership(&mut self, new_owner: Address) {
                    self.owner = Some(new_owner);
                }
            }
        };
        let trait_impl = TraitImplInfo {
            trait_name: "OwnableTrait".to_string(),
            impl_block: &impl_block,
            expose_list: vec!["owner".to_string(), "transfer_ownership".to_string()],
        };
        let result = trait_methods(&trait_impl);
        assert!(result.is_ok());
        let functions = result.unwrap();
        assert_eq!(functions.len(), 2);
    }

    #[test]
    fn test_trait_methods_missing_method() {
        let impl_block: ItemImpl = syn::parse_quote! {
            #[contract(expose = [owner, nonexistent])]
            impl OwnableTrait for MyContract {
                fn owner(&self) -> Option<Address> { self.owner }
            }
        };
        let trait_impl = TraitImplInfo {
            trait_name: "OwnableTrait".to_string(),
            impl_block: &impl_block,
            expose_list: vec!["owner".to_string(), "nonexistent".to_string()],
        };
        let result = trait_methods(&trait_impl);
        let Err(err) = result else {
            panic!("expected error for missing method");
        };
        assert!(err.to_string().contains("nonexistent"));
        assert!(err.to_string().contains("not found"));
    }
}
