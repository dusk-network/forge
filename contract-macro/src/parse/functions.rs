// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Function-shape parsing: turning impl blocks and their methods into
//! `FunctionInfo` IR (parameters, input/output types, receiver, doc, feeds).

use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    Attribute, Expr, ExprLit, FnArg, ImplItem, ImplItemFn, ItemImpl, Lit, Pat, ReturnType, Type,
    Visibility,
};

use crate::parse::{directives, events};
use crate::{FunctionInfo, ParameterInfo, Receiver, TraitImplInfo, validate};

/// Check if a method body is empty (just `{}`).
///
/// Empty bodies in trait impls signal "use the default implementation,
/// I'm just providing the signature for wrapper generation".
pub(super) fn has_empty_body(method: &ImplItemFn) -> bool {
    method.block.stmts.is_empty()
}

/// Extract the receiver type from a method signature.
pub(super) fn extract_receiver(method: &ImplItemFn) -> Receiver {
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
pub(super) fn extract_doc_comment(attrs: &[Attribute]) -> Option<String> {
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

/// Validate feed-related attributes for a method.
///
/// Checks that:
/// 1. There is at most one `abi::feed()` call site in the function
/// 2. If `abi::feed()` is used, the `#[contract(feeds = "Type")]` attribute is
///    present
/// 3. If present, the feeds type matches the fed expression (tuple vs
///    non-tuple)
///
/// Returns an error if validation fails.
fn validate_feeds(
    method: &ImplItemFn,
    name: &Ident,
    feed_type: Option<&TokenStream2>,
) -> Result<(), syn::Error> {
    let feed_exprs = events::get_feed_exprs(method);

    // Check for multiple feed call sites
    if feed_exprs.len() > 1 {
        // Deduplicate to show unique expressions
        let mut unique_exprs: Vec<_> = feed_exprs.clone();
        unique_exprs.sort();
        unique_exprs.dedup();

        let exprs_list = unique_exprs.join("`, `");
        return Err(syn::Error::new_spanned(
            &method.sig,
            format!(
                "method `{name}` has multiple `abi::feed()` calls; \
                 only one feed call site is allowed per function (found: `{exprs_list}`)"
            ),
        ));
    }

    if let Some(ft) = &feed_type {
        // Has feeds attribute - validate it matches the expressions
        if let Some(mismatch_msg) = events::validate_feed_type_match(&ft.to_string(), &feed_exprs) {
            return Err(syn::Error::new_spanned(&method.sig, mismatch_msg));
        }
    } else if !feed_exprs.is_empty() {
        // Uses abi::feed() but missing feeds attribute
        return Err(syn::Error::new_spanned(
            &method.sig,
            format!(
                "method `{name}` uses `abi::feed()` but is missing `#[contract(feeds = \"Type\")]` attribute; \
                 feeds: `{}`",
                feed_exprs[0]
            ),
        ));
    }

    Ok(())
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
            let feed_type = directives::extract_feeds_attribute(&method.attrs);
            let receiver = extract_receiver(method);

            // Check for method-level emits attribute
            let method_events = events::method_emits(&method.attrs);
            let has_method_emits = !method_events.is_empty();

            // For trait methods:
            // - Default impl (empty body): check if emits attribute registered on method
            // - Non-default impl: check body for emit calls
            let has_emit_call = if is_default_impl {
                has_method_emits
            } else {
                events::method_has_emit_call(method)
            };
            let suppressed = directives::event_suppressed(&method.attrs);

            // Validate feed-related attributes
            // (only check non-empty bodies since empty bodies delegate to trait defaults)
            if !is_default_impl {
                validate_feeds(method, &name, feed_type.as_ref())?;
            }

            // Validate that mutating methods emit events
            validate::method_emits_event(method, has_emit_call, suppressed, has_method_emits)?;

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
            let feed_type = directives::extract_feeds_attribute(&method.attrs);
            let receiver = extract_receiver(method);
            let has_emit_call = events::method_has_emit_call(method);
            let suppressed = directives::event_suppressed(&method.attrs);
            let has_method_emits = !events::method_emits(&method.attrs).is_empty();

            // Validate feed-related attributes
            validate_feeds(method, &name, feed_type.as_ref())?;

            // Validate that mutating methods emit events
            validate::method_emits_event(method, has_emit_call, suppressed, has_method_emits)?;

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
fn parameters(method: &ImplItemFn) -> Vec<ParameterInfo> {
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
fn input_type(params: &[ParameterInfo]) -> TokenStream2 {
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
fn output_type(ret: &ReturnType) -> (TokenStream2, bool) {
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

    // ========================================================================
    // output_type tests
    // ========================================================================

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

    // ========================================================================
    // parameters tests
    // ========================================================================

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

    // ========================================================================
    // trait_methods / public_methods tests
    // ========================================================================

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
                // Method-level emits attribute for trait default impl
                #[contract(emits = [(OwnershipTransferred::TOPIC, OwnershipTransferred)])]
                fn transfer_ownership(&mut self, new_owner: Address) {}
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
    fn test_public_methods_delegating_with_emits() {
        // Inherent method with an emit-free body but an `emits` attribute
        // (delegates to a helper) — the new strict check must accept it.
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                #[contract(emits = [(Resolved::TOPIC, Resolved)])]
                pub fn resolve(&mut self) {
                    self.core.resolve();
                }
            }
        };
        let functions = match public_methods(&impl_block) {
            Ok(functions) => functions,
            Err(err) => panic!("expected success, got: {err}"),
        };
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name.to_string(), "resolve");
    }

    #[test]
    fn test_public_methods_delegating_without_emits_errors() {
        // Same shape without the `emits` attribute must still fail the strict
        // mutating-method check.
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn resolve(&mut self) {
                    self.core.resolve();
                }
            }
        };
        let Err(err) = public_methods(&impl_block) else {
            panic!("expected error for delegating method without emits");
        };
        assert!(err.to_string().contains("emits no events"));
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

    // ========================================================================
    // Feed validation tests
    // ========================================================================

    #[test]
    fn test_validate_feeds_missing_attribute() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn stream_data(&self) {
                abi::feed(42u64);
            }
        };
        let name = format_ident!("stream_data");
        let result = validate_feeds(&method, &name, None);

        let Err(err) = result else {
            panic!("expected error for missing feeds attribute");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("missing"),
            "error should mention 'missing': {msg}"
        );
        assert!(msg.contains("feeds"), "error should mention 'feeds': {msg}");
        assert!(
            msg.contains("42u64"),
            "error should show fed expression: {msg}"
        );
    }

    #[test]
    fn test_validate_feeds_multiple_calls() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn stream_multiple(&self) {
                abi::feed(self.items[0]);
                abi::feed(self.items[1]);
            }
        };
        let name = format_ident!("stream_multiple");
        let feed_type: TokenStream2 = quote! { u64 };
        let result = validate_feeds(&method, &name, Some(&feed_type));

        let Err(err) = result else {
            panic!("expected error for multiple feed calls");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("multiple"),
            "error should mention 'multiple': {msg}"
        );
        assert!(
            msg.contains("abi::feed()"),
            "error should mention 'abi::feed()': {msg}"
        );
    }

    #[test]
    fn test_validate_feeds_tuple_mismatch() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn stream_mismatch(&self) {
                abi::feed(42u64);
            }
        };
        let name = format_ident!("stream_mismatch");
        let feed_type: TokenStream2 = quote! { (u64, u64) };
        let result = validate_feeds(&method, &name, Some(&feed_type));

        let Err(err) = result else {
            panic!("expected error for tuple mismatch");
        };
        let msg = err.to_string();
        assert!(msg.contains("tuple"), "error should mention 'tuple': {msg}");
        assert!(
            msg.contains("42u64"),
            "error should show fed expression: {msg}"
        );
    }

    #[test]
    fn test_validate_feeds_valid_with_attribute() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn stream_valid(&self) {
                abi::feed(42u64);
            }
        };
        let name = format_ident!("stream_valid");
        let feed_type: TokenStream2 = quote! { u64 };
        let result = validate_feeds(&method, &name, Some(&feed_type));

        assert!(result.is_ok(), "valid feeds usage should not error");
    }

    #[test]
    fn test_validate_feeds_no_feed_no_attribute() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn regular_method(&self) -> u64 {
                42
            }
        };
        let name = format_ident!("regular_method");
        let result = validate_feeds(&method, &name, None);

        assert!(
            result.is_ok(),
            "method without abi::feed() should not require attribute"
        );
    }

    #[test]
    fn test_validate_feeds_in_loop() {
        // abi::feed() inside a loop is still detected as a single call site
        let method: ImplItemFn = syn::parse_quote! {
            pub fn stream_in_loop(&self) {
                for item in &self.items {
                    abi::feed(*item);
                }
            }
        };
        let name = format_ident!("stream_in_loop");
        let feed_type: TokenStream2 = quote! { u64 };
        let result = validate_feeds(&method, &name, Some(&feed_type));

        // A single feed call inside a loop is valid
        assert!(result.is_ok(), "single feed call in loop should be valid");
    }

    #[test]
    fn test_validate_feeds_multiple_in_loop() {
        // Multiple abi::feed() calls even inside a loop should error
        let method: ImplItemFn = syn::parse_quote! {
            pub fn stream_multiple_in_loop(&self) {
                for item in &self.items {
                    abi::feed(item.id);
                    abi::feed(item.value);
                }
            }
        };
        let name = format_ident!("stream_multiple_in_loop");
        let feed_type: TokenStream2 = quote! { u64 };
        let result = validate_feeds(&method, &name, Some(&feed_type));

        let Err(err) = result else {
            panic!("expected error for multiple feed calls in loop");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("multiple"),
            "error should mention 'multiple': {msg}"
        );
    }

    #[test]
    fn test_validate_feeds_in_if_block() {
        // abi::feed() inside an if block is still detected
        let method: ImplItemFn = syn::parse_quote! {
            pub fn stream_conditional(&self) {
                if self.is_ready {
                    abi::feed(self.data);
                }
            }
        };
        let name = format_ident!("stream_conditional");
        let feed_type: TokenStream2 = quote! { u64 };
        let result = validate_feeds(&method, &name, Some(&feed_type));

        assert!(
            result.is_ok(),
            "single feed call in if block should be valid"
        );
    }

    #[test]
    fn test_validate_feeds_in_multiple_branches() {
        // abi::feed() in multiple if/else branches counts as multiple call sites
        let method: ImplItemFn = syn::parse_quote! {
            pub fn stream_branches(&self) {
                if self.use_a {
                    abi::feed(self.a);
                } else {
                    abi::feed(self.b);
                }
            }
        };
        let name = format_ident!("stream_branches");
        let feed_type: TokenStream2 = quote! { u64 };
        let result = validate_feeds(&method, &name, Some(&feed_type));

        let Err(err) = result else {
            panic!("expected error for feed calls in multiple branches");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("multiple"),
            "error should mention 'multiple': {msg}"
        );
    }

    #[test]
    fn test_validate_feeds_tuple_to_non_tuple_mismatch() {
        // Feed type is tuple but expression is non-tuple
        let method: ImplItemFn = syn::parse_quote! {
            pub fn stream_wants_tuple(&self) {
                abi::feed(42u64);
            }
        };
        let name = format_ident!("stream_wants_tuple");
        let feed_type: TokenStream2 = quote! { (u64, String) };
        let result = validate_feeds(&method, &name, Some(&feed_type));

        let Err(err) = result else {
            panic!("expected error for tuple mismatch");
        };
        let msg = err.to_string();
        assert!(msg.contains("tuple"), "error should mention 'tuple': {msg}");
    }

    #[test]
    fn test_validate_feeds_non_tuple_to_tuple_mismatch() {
        // Feed type is non-tuple but expression is tuple
        let method: ImplItemFn = syn::parse_quote! {
            pub fn stream_sends_tuple(&self) {
                abi::feed((self.id, self.value));
            }
        };
        let name = format_ident!("stream_sends_tuple");
        let feed_type: TokenStream2 = quote! { u64 };
        let result = validate_feeds(&method, &name, Some(&feed_type));

        let Err(err) = result else {
            panic!("expected error for tuple mismatch");
        };
        let msg = err.to_string();
        assert!(msg.contains("tuple"), "error should mention 'tuple': {msg}");
    }

    // ========================================================================
    // extract_doc_comment tests
    // ========================================================================

    #[test]
    fn test_extract_doc_comment_single_line() {
        let attrs: Vec<Attribute> = vec![syn::parse_quote!(#[doc = " First line."])];

        let doc = extract_doc_comment(&attrs);
        assert!(doc.is_some());
        assert_eq!(doc.unwrap(), "First line.");
    }

    #[test]
    fn test_extract_doc_comment_multiple_lines() {
        let attrs: Vec<Attribute> = vec![
            syn::parse_quote!(#[doc = " First line."]),
            syn::parse_quote!(#[doc = " Second line."]),
        ];

        let doc = extract_doc_comment(&attrs);
        assert!(doc.is_some());
        let doc = doc.unwrap();
        assert!(doc.contains("First line"));
        assert!(doc.contains("Second line"));
    }

    #[test]
    fn test_extract_doc_comment_none() {
        let attrs: Vec<Attribute> = vec![syn::parse_quote!(#[inline])];

        let doc = extract_doc_comment(&attrs);
        assert!(doc.is_none());
    }

    #[test]
    fn test_extract_doc_comment_empty() {
        let attrs: Vec<Attribute> = vec![];

        let doc = extract_doc_comment(&attrs);
        assert!(doc.is_none());
    }

    #[test]
    fn test_extract_doc_comment_mixed_attrs() {
        let attrs: Vec<Attribute> = vec![
            syn::parse_quote!(#[inline]),
            syn::parse_quote!(#[doc = " The doc comment."]),
            syn::parse_quote!(#[allow(unused)]),
        ];

        let doc = extract_doc_comment(&attrs);
        assert!(doc.is_some());
        assert_eq!(doc.unwrap(), "The doc comment.");
    }
}
