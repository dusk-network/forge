// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Data driver module generation.
//!
//! Generates a `data_driver` module at crate root level containing a `Driver`
//! struct that implements the `ConvertibleContract` trait from
//! `dusk-data-driver`.
//!
//! The module is feature-gated with `#[cfg(feature = "data-driver")]` and uses
//! fully-qualified type paths resolved at extraction time.

use std::collections::HashSet;

use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};

use crate::resolve::TypeMap;
use crate::{CustomDataDriverHandler, DataDriverRole, EventInfo, FunctionInfo, ImportInfo};

/// Canonical handler signature for a given role.
///
/// Both the dispatch code in this module (which splices `handler(arg)` calls
/// into the generated match arms) and the compile-time handler validator
/// consume this — changes to the dispatch shape must go through here so the
/// two agree.
pub(crate) struct HandlerSignature {
    /// The handler's sole argument type, e.g. `&str` or `&[u8]`.
    pub arg_type: TokenStream2,
    /// The handler's return type, e.g. `Result<alloc::vec::Vec<u8>, …>`.
    pub return_type: TokenStream2,
}

/// Canonical signature per role.
///
/// The `arg_type` reflects the name used in the dispatch (`json` is `&str`,
/// `rkyv` is `&[u8]`). The `return_type` reflects the trait method that owns
/// each match arm.
pub(crate) fn handler_signature(role: DataDriverRole) -> HandlerSignature {
    match role {
        DataDriverRole::EncodeInput => HandlerSignature {
            arg_type: quote!(&str),
            return_type: quote!(Result<alloc::vec::Vec<u8>, dusk_data_driver::Error>),
        },
        DataDriverRole::DecodeInput | DataDriverRole::DecodeOutput => HandlerSignature {
            arg_type: quote!(&[u8]),
            return_type: quote!(Result<dusk_data_driver::JsonValue, dusk_data_driver::Error>),
        },
    }
}

/// Human-readable role name, matching the attribute the user writes.
pub(crate) fn role_name(role: DataDriverRole) -> &'static str {
    match role {
        DataDriverRole::EncodeInput => "encode_input",
        DataDriverRole::DecodeInput => "decode_input",
        DataDriverRole::DecodeOutput => "decode_output",
    }
}

/// Render the canonical handler signature for display in diagnostics,
/// e.g. `fn(&str) -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error>`.
pub(crate) fn handler_signature_display(role: DataDriverRole) -> String {
    let sig = handler_signature(role);
    let arg = pretty_tokens(&sig.arg_type);
    let ret = pretty_tokens(&sig.return_type);
    format!("fn({arg}) -> {ret}")
}

/// Normalized token string — collapses whitespace differences introduced by
/// `quote!` so compared signatures are stable regardless of how the user
/// spaced their handler's types.
pub(crate) fn normalize_tokens_string(tokens: &TokenStream2) -> String {
    tokens
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Pretty-printed form of a type token stream for human-readable diagnostics.
///
/// `TokenStream::to_string` emits `& str` / `Result < T , E >` with spaces
/// that rustc's type printer doesn't use — this trims them back down so the
/// signature displayed to the user matches what they would write in code.
pub(crate) fn pretty_tokens(tokens: &TokenStream2) -> String {
    normalize_tokens_string(tokens)
        .replace(" :: ", "::")
        .replace(" < ", "<")
        .replace(" <", "<")
        .replace(" > ", ">")
        .replace(" >", ">")
        .replace("& ", "&")
        .replace(" ,", ",")
}

/// Generate the runtime error arm body for an `is_custom` function at the
/// given dispatch site.
///
/// Produces a `Result::Err` with a role-tailored message that names both the
/// role (so the user knows which handler is missing) and the expected
/// handler signature in concrete types (so the user can fix the handler from
/// the error alone).
fn missing_handler_arm(fn_name: &str, role: DataDriverRole) -> TokenStream2 {
    let role_str = role_name(role);
    let sig_str = handler_signature_display(role);
    quote! {
        #fn_name => Err(dusk_data_driver::Error::Unsupported(
            alloc::format!(
                "missing {} handler for `{}`; expected handler signature: {}",
                #role_str, #fn_name, #sig_str
            )
        ))
    }
}

/// Build `use` items that mirror the contract module's imports needed by
/// custom handlers, to be spliced into the generated `data_driver` submodule.
///
/// Only imports referenced by handler tokens (signature or body) are emitted,
/// to keep the submodule from inheriting contract-only imports (e.g. ABI
/// types feature-gated out of the data-driver build). Each entry becomes
/// `use <path> as <name>;` when the path's last segment differs from the
/// name (i.e. the user wrote `use X as Y;`), and `use <path>;` otherwise —
/// so handlers moved into the submodule resolve the same short names they
/// resolved in the outer module, for both signature and body.
fn reemit_imports(
    imports: &[ImportInfo],
    handlers: &[CustomDataDriverHandler],
) -> Vec<TokenStream2> {
    let handler_idents = collect_handler_identifiers(handlers);

    imports
        .iter()
        .filter(|import| handler_idents.contains(&import.name))
        .filter_map(|import| {
            let path: syn::Path = syn::parse_str(&import.path).ok()?;
            let last_seg = path.segments.last()?.ident.to_string();
            let item = if last_seg == import.name {
                quote! { use #path; }
            } else {
                let alias: syn::Ident = syn::parse_str(&import.name).ok()?;
                quote! { use #path as #alias; }
            };
            Some(item)
        })
        .collect()
}

/// Collect every identifier that appears in any handler's tokens.
///
/// Used to filter the contract module's imports down to those the handlers
/// actually reference. A handler that uses `Error::from(…)` contributes
/// `Error` (plus `from`, which no import will match); an import named
/// `BTreeMap` that no handler mentions is skipped.
fn collect_handler_identifiers(handlers: &[CustomDataDriverHandler]) -> HashSet<String> {
    use proc_macro2::TokenTree;

    fn walk(stream: TokenStream2, out: &mut HashSet<String>) {
        for tree in stream {
            match tree {
                TokenTree::Ident(ident) => {
                    out.insert(ident.to_string());
                }
                TokenTree::Group(group) => walk(group.stream(), out),
                _ => {}
            }
        }
    }

    let mut idents = HashSet::new();
    for handler in handlers {
        walk(handler.func.to_token_stream(), &mut idents);
    }
    idents
}

/// Generate the `data_driver` module at crate root level.
pub(crate) fn module(
    imports: &[ImportInfo],
    type_map: &TypeMap,
    functions: &[FunctionInfo],
    events: &[EventInfo],
    custom_handlers: &[CustomDataDriverHandler],
) -> TokenStream2 {
    let encode_input_arms = generate_encode_input_arms(functions, type_map, custom_handlers);
    let decode_input_arms = generate_decode_input_arms(functions, type_map, custom_handlers);
    let decode_output_arms = generate_decode_output_arms(functions, type_map, custom_handlers);
    let decode_event_arms = generate_decode_event_arms(events, type_map);

    // Collect custom handler functions to include in the module
    let custom_handler_fns: Vec<_> = custom_handlers.iter().map(|h| &h.func).collect();

    // Re-emit the contract module's `use` items inside the generated submodule
    // so custom handlers — spliced verbatim from the contract module — resolve
    // the same short-name paths they did at their original site (handler
    // signature *and* body).
    let contract_imports = reemit_imports(imports, custom_handlers);

    quote! {
        /// Auto-generated data driver module.
        ///
        /// This module provides a `Driver` struct implementing `ConvertibleContract`
        /// for encoding/decoding contract function inputs, outputs, and events.
        #[cfg(feature = "data-driver")]
        pub mod data_driver {
            #![allow(unused_imports)]

            extern crate alloc;

            // Imports re-emitted from the contract module so that spliced
            // custom handler functions resolve the same short-name paths here
            // as they did at their original definition site.
            //
            // The macro-generated scaffolding below uses fully-qualified paths
            // (`alloc::vec::Vec`, `alloc::string::String`) so user imports of
            // `Vec` / `String` won't collide with a preluded one we control.
            #(#contract_imports)*

            // Custom handler functions moved from the contract module
            #(#custom_handler_fns)*

            /// Auto-generated contract driver.
            #[derive(Default)]
            pub struct Driver;

            #[allow(clippy::match_same_arms)]
            impl dusk_data_driver::ConvertibleContract for Driver {
                fn encode_input_fn(
                    &self,
                    fn_name: &str,
                    json: &str,
                ) -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error> {
                    match fn_name {
                        #(#encode_input_arms,)*
                        name => Err(dusk_data_driver::Error::Unsupported(
                            alloc::format!("encode_input: unknown fn {name}")
                        ))
                    }
                }

                fn decode_input_fn(
                    &self,
                    fn_name: &str,
                    rkyv: &[u8],
                ) -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error> {
                    match fn_name {
                        #(#decode_input_arms,)*
                        name => Err(dusk_data_driver::Error::Unsupported(
                            alloc::format!("decode_input: unknown fn {name}")
                        ))
                    }
                }

                fn decode_output_fn(
                    &self,
                    fn_name: &str,
                    rkyv: &[u8],
                ) -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error> {
                    match fn_name {
                        #(#decode_output_arms,)*
                        name => Err(dusk_data_driver::Error::Unsupported(
                            alloc::format!("decode_output: unknown fn {name}")
                        ))
                    }
                }

                fn decode_event(
                    &self,
                    event_name: &str,
                    rkyv: &[u8],
                ) -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error> {
                    match event_name {
                        #(#decode_event_arms,)*
                        name => Err(dusk_data_driver::Error::Unsupported(
                            alloc::format!("decode_event: unknown event {name}")
                        ))
                    }
                }

                fn get_schema(&self) -> alloc::string::String {
                    super::CONTRACT_SCHEMA.to_json()
                }
            }

            // WASM entrypoint for the data-driver
            #[cfg(target_family = "wasm")]
            dusk_data_driver::generate_wasm_entrypoint!(Driver);
        }
    }
}

/// Get the resolved type path from the `type_map`, or return the original if
/// not found.
fn get_resolved_type(ty: &TokenStream2, type_map: &TypeMap) -> TokenStream2 {
    let key = ty.to_string();
    if let Some(resolved) = type_map.get(&key) {
        // Parse the resolved string back into tokens as a Type (not Path, since tuples
        // aren't paths)
        if let Ok(resolved_type) = syn::parse_str::<syn::Type>(resolved) {
            return quote! { #resolved_type };
        }
    }
    // Fallback to original
    ty.clone()
}

/// Generate match arms for `encode_input_fn`.
fn generate_encode_input_arms(
    functions: &[FunctionInfo],
    type_map: &TypeMap,
    custom_handlers: &[CustomDataDriverHandler],
) -> Vec<TokenStream2> {
    let mut arms: Vec<TokenStream2> = functions
        .iter()
        .map(|f| {
            let name_str = f.name.to_string();
            let input_type = get_resolved_type(&f.input_type, type_map);

            if f.is_custom {
                missing_handler_arm(&name_str, DataDriverRole::EncodeInput)
            } else {
                quote! {
                    #name_str => dusk_data_driver::json_to_rkyv::<#input_type>(json)
                }
            }
        })
        .collect();

    // Add custom handler arms
    for handler in custom_handlers {
        if handler.role == DataDriverRole::EncodeInput {
            let fn_name_str = &handler.fn_name;
            let handler_fn_name = &handler.func.sig.ident;
            arms.push(quote! {
                #fn_name_str => #handler_fn_name(json)
            });
        }
    }

    arms
}

/// Generate match arms for `decode_input_fn`.
fn generate_decode_input_arms(
    functions: &[FunctionInfo],
    type_map: &TypeMap,
    custom_handlers: &[CustomDataDriverHandler],
) -> Vec<TokenStream2> {
    let mut arms: Vec<TokenStream2> = functions
        .iter()
        .map(|f| {
            let name_str = f.name.to_string();
            let input_type = get_resolved_type(&f.input_type, type_map);

            if f.is_custom {
                missing_handler_arm(&name_str, DataDriverRole::DecodeInput)
            } else {
                quote! {
                    #name_str => dusk_data_driver::rkyv_to_json::<#input_type>(rkyv)
                }
            }
        })
        .collect();

    // Add custom handler arms
    for handler in custom_handlers {
        if handler.role == DataDriverRole::DecodeInput {
            let fn_name_str = &handler.fn_name;
            let handler_fn_name = &handler.func.sig.ident;
            arms.push(quote! {
                #fn_name_str => #handler_fn_name(rkyv)
            });
        }
    }

    arms
}

/// Generate match arms for `decode_output_fn`.
///
/// When a function has a `feed_type` (from `#[contract(feeds = "Type")]`),
/// that type is used for decoding instead of the return type. This handles
/// functions that stream data via `abi::feed()` rather than returning directly.
fn generate_decode_output_arms(
    functions: &[FunctionInfo],
    type_map: &TypeMap,
    custom_handlers: &[CustomDataDriverHandler],
) -> Vec<TokenStream2> {
    let mut arms: Vec<TokenStream2> = functions
        .iter()
        .map(|f| {
            let name_str = f.name.to_string();

            // Use feed_type if present, otherwise use output_type
            let (decode_type, type_str) = if let Some(feed_type) = &f.feed_type {
                (
                    get_resolved_type(feed_type, type_map),
                    feed_type.to_string(),
                )
            } else {
                (
                    get_resolved_type(&f.output_type, type_map),
                    f.output_type.to_string(),
                )
            };

            if f.is_custom {
                missing_handler_arm(&name_str, DataDriverRole::DecodeOutput)
            } else if type_str == "()" {
                quote! {
                    #name_str => Ok(dusk_data_driver::JsonValue::Null)
                }
            } else if type_str == "u64" {
                quote! {
                    #name_str => dusk_data_driver::rkyv_to_json_u64(rkyv)
                }
            } else {
                quote! {
                    #name_str => dusk_data_driver::rkyv_to_json::<#decode_type>(rkyv)
                }
            }
        })
        .collect();

    // Add custom handler arms
    for handler in custom_handlers {
        if handler.role == DataDriverRole::DecodeOutput {
            let fn_name_str = &handler.fn_name;
            let handler_fn_name = &handler.func.sig.ident;
            arms.push(quote! {
                #fn_name_str => #handler_fn_name(rkyv)
            });
        }
    }

    arms
}

/// Generate match arms for `decode_event`.
fn generate_decode_event_arms(events: &[EventInfo], type_map: &TypeMap) -> Vec<TokenStream2> {
    events
        .iter()
        .filter_map(|e| {
            let topic_str = &e.topic;
            let data_type = get_resolved_type(&e.data_type, type_map);

            // Get the resolved topic path from the type_map
            let resolved_topic = type_map
                .get(topic_str)
                .map_or(topic_str.clone(), Clone::clone);

            // Try to parse the resolved topic as a path for constant resolution
            if let Ok(topic_path) = syn::parse_str::<syn::Path>(&resolved_topic) {
                // Skip variable references (single lowercase identifier)
                if topic_path.segments.len() == 1 {
                    let name = topic_path.segments[0].ident.to_string();
                    if name.starts_with(char::is_lowercase) {
                        return None;
                    }
                }
                Some(quote! {
                    #topic_path => dusk_data_driver::rkyv_to_json::<#data_type>(rkyv)
                })
            } else {
                Some(quote! {
                    #resolved_topic => dusk_data_driver::rkyv_to_json::<#data_type>(rkyv)
                })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use quote::format_ident;

    use super::*;
    use crate::Receiver;

    /// Normalize token stream to a string with consistent whitespace for
    /// comparison.
    fn normalize_tokens(tokens: TokenStream2) -> String {
        tokens
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Create a basic `FunctionInfo` for testing.
    fn make_function(
        name: &str,
        input: TokenStream2,
        output: TokenStream2,
        is_custom: bool,
    ) -> FunctionInfo {
        FunctionInfo {
            name: format_ident!("{}", name),
            doc: None,
            params: vec![],
            input_type: input,
            output_type: output,
            is_custom,
            returns_ref: false,
            receiver: Receiver::Ref,
            trait_name: None,
            feed_type: None,
        }
    }

    /// Create an `EventInfo` for testing.
    fn make_event(topic: &str, data_type: TokenStream2) -> EventInfo {
        EventInfo {
            topic: topic.to_string(),
            data_type,
        }
    }

    /// Create a `CustomDataDriverHandler` for testing.
    fn make_custom_handler(
        fn_name: &str,
        role: DataDriverRole,
        handler_name: &str,
    ) -> CustomDataDriverHandler {
        // Build the function using the handler_name identifier
        let handler_ident = format_ident!("{}", handler_name);
        let func: syn::ItemFn = syn::parse_quote! {
            fn #handler_ident(_input: &str) -> Result<Vec<u8>, Error> {
                Ok(vec![])
            }
        };

        CustomDataDriverHandler {
            fn_name: fn_name.to_string(),
            role,
            func,
        }
    }

    // =========================================================================
    // get_resolved_type tests
    // =========================================================================

    #[test]
    fn test_get_resolved_type_found_in_map() {
        let mut type_map = HashMap::new();
        type_map.insert("Address".to_string(), "my_crate::Address".to_string());

        let ty = quote! { Address };
        let resolved = get_resolved_type(&ty, &type_map);

        assert_eq!(normalize_tokens(resolved), "my_crate :: Address");
    }

    #[test]
    fn test_get_resolved_type_not_in_map() {
        let type_map = HashMap::new();

        let ty = quote! { u64 };
        let resolved = get_resolved_type(&ty, &type_map);

        assert_eq!(normalize_tokens(resolved), "u64");
    }

    #[test]
    fn test_get_resolved_type_complex_path() {
        let mut type_map = HashMap::new();
        type_map.insert("Deposit".to_string(), "my_crate::Deposit".to_string());

        let ty = quote! { Deposit };
        let resolved = get_resolved_type(&ty, &type_map);

        assert_eq!(normalize_tokens(resolved), "my_crate :: Deposit");
    }

    // =========================================================================
    // generate_encode_input_arms tests
    // =========================================================================

    #[test]
    fn test_encode_input_simple_type() {
        let mut type_map = HashMap::new();
        type_map.insert("Address".to_string(), "my_crate::Address".to_string());

        let functions = vec![make_function(
            "init",
            quote! { Address },
            quote! { () },
            false,
        )];
        let arms = generate_encode_input_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"init\""), "Should contain function name");
        assert!(arm_str.contains("json_to_rkyv"), "Should use json_to_rkyv");
        assert!(
            arm_str.contains("my_crate :: Address"),
            "Should use resolved type"
        );
    }

    #[test]
    fn test_encode_input_unit_type() {
        let type_map = HashMap::new();

        let functions = vec![make_function(
            "is_paused",
            quote! { () },
            quote! { bool },
            false,
        )];
        let arms = generate_encode_input_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"is_paused\""));
        assert!(arm_str.contains("json_to_rkyv :: < () >"));
    }

    #[test]
    fn test_encode_input_tuple_type() {
        let mut type_map = HashMap::new();
        // Key must match the exact token stream string representation
        type_map.insert(
            "(Address , u64)".to_string(),
            "(my_crate::Address, u64)".to_string(),
        );

        let functions = vec![make_function(
            "transfer",
            quote! { (Address, u64) },
            quote! { () },
            false,
        )];
        let arms = generate_encode_input_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"transfer\""));
        assert!(arm_str.contains("json_to_rkyv"));
        // Verify the resolved tuple type is used
        assert!(
            arm_str.contains("my_crate :: Address"),
            "Should use resolved type in tuple: {}",
            arm_str
        );
    }

    #[test]
    fn test_encode_input_custom_returns_error() {
        let type_map = HashMap::new();

        let functions = vec![make_function(
            "custom_fn",
            quote! { CustomType },
            quote! { Vec<u8> },
            true, // is_custom = true
        )];
        let arms = generate_encode_input_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"custom_fn\""));
        assert!(arm_str.contains("Err"));
        assert!(arm_str.contains("Unsupported"));
        // The generated error names the role so a user seeing it at runtime
        // can tell which of the three sites is missing a handler.
        assert!(
            arm_str.contains("\"encode_input\""),
            "error should name the encode_input role: {arm_str}"
        );
        // The generated error includes the canonical handler signature in
        // concrete types so the user can fix the handler from the message.
        assert!(
            arm_str.contains(&handler_signature_display(DataDriverRole::EncodeInput)),
            "error should include the encode_input signature verbatim: {arm_str}"
        );
    }

    #[test]
    fn test_encode_input_with_custom_handler() {
        let type_map = HashMap::new();
        let functions = vec![]; // No regular functions

        let custom_handlers = vec![make_custom_handler(
            "extra_data",
            DataDriverRole::EncodeInput,
            "encode_extra_data",
        )];

        let arms = generate_encode_input_arms(&functions, &type_map, &custom_handlers);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"extra_data\""));
        assert!(arm_str.contains("encode_extra_data"));
        assert!(arm_str.contains("(json)"));
    }

    #[test]
    fn test_encode_input_multiple_functions() {
        let type_map = HashMap::new();

        let functions = vec![
            make_function("pause", quote! { () }, quote! { () }, false),
            make_function("unpause", quote! { () }, quote! { () }, false),
            make_function("init", quote! { Address }, quote! { () }, false),
        ];
        let arms = generate_encode_input_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 3);

        // Verify each function is present in the generated arms
        let all_arms: String = arms.iter().map(|a| normalize_tokens(a.clone())).collect();
        assert!(all_arms.contains("\"pause\""), "Should contain pause");
        assert!(all_arms.contains("\"unpause\""), "Should contain unpause");
        assert!(all_arms.contains("\"init\""), "Should contain init");
    }

    // =========================================================================
    // generate_decode_input_arms tests
    // =========================================================================

    #[test]
    fn test_decode_input_simple_type() {
        let mut type_map = HashMap::new();
        type_map.insert("Deposit".to_string(), "my_crate::Deposit".to_string());

        let functions = vec![make_function(
            "deposit",
            quote! { Deposit },
            quote! { () },
            false,
        )];
        let arms = generate_decode_input_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"deposit\""));
        assert!(arm_str.contains("rkyv_to_json"));
        assert!(arm_str.contains("my_crate :: Deposit"));
    }

    #[test]
    fn test_decode_input_custom_returns_error() {
        let type_map = HashMap::new();

        let functions = vec![make_function(
            "custom_fn",
            quote! { CustomType },
            quote! { () },
            true,
        )];
        let arms = generate_decode_input_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("Err"));
        assert!(
            arm_str.contains("\"decode_input\""),
            "error should name the decode_input role: {arm_str}"
        );
        assert!(
            arm_str.contains(&handler_signature_display(DataDriverRole::DecodeInput)),
            "error should include the decode_input signature verbatim: {arm_str}"
        );
    }

    #[test]
    fn test_decode_input_with_custom_handler() {
        let type_map = HashMap::new();
        let functions = vec![];

        let custom_handlers = vec![make_custom_handler(
            "extra_data",
            DataDriverRole::DecodeInput,
            "decode_extra_input",
        )];

        let arms = generate_decode_input_arms(&functions, &type_map, &custom_handlers);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"extra_data\""));
        assert!(arm_str.contains("decode_extra_input"));
        assert!(arm_str.contains("(rkyv)"));
    }

    #[test]
    fn test_decode_input_tuple_type() {
        let mut type_map = HashMap::new();
        // Key must match the exact token stream string representation
        type_map.insert(
            "(Address , MyAddr , u64)".to_string(),
            "(my_crate::Address, my_crate::MyAddr, u64)".to_string(),
        );

        let functions = vec![make_function(
            "transfer_with_fee",
            quote! { (Address, MyAddr, u64) },
            quote! { () },
            false,
        )];
        let arms = generate_decode_input_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"transfer_with_fee\""));
        assert!(arm_str.contains("rkyv_to_json"));
        // Verify the resolved tuple type is used
        assert!(
            arm_str.contains("my_crate :: Address"),
            "Should use resolved Address type in tuple: {}",
            arm_str
        );
        assert!(
            arm_str.contains("my_crate :: MyAddr"),
            "Should use resolved MyAddr type in tuple: {}",
            arm_str
        );
    }

    #[test]
    fn test_custom_handler_wrong_role_not_included_in_encode() {
        let type_map = HashMap::new();
        let functions = vec![];

        // DecodeOutput handler should NOT appear in encode_input_arms
        let custom_handlers = vec![make_custom_handler(
            "extra_data",
            DataDriverRole::DecodeOutput,
            "decode_extra_output",
        )];

        let arms = generate_encode_input_arms(&functions, &type_map, &custom_handlers);

        assert_eq!(
            arms.len(),
            0,
            "DecodeOutput handler should not appear in encode_input_arms"
        );
    }

    #[test]
    fn test_custom_handler_wrong_role_not_included_in_decode_input() {
        let type_map = HashMap::new();
        let functions = vec![];

        // EncodeInput handler should NOT appear in decode_input_arms
        let custom_handlers = vec![make_custom_handler(
            "extra_data",
            DataDriverRole::EncodeInput,
            "encode_extra_data",
        )];

        let arms = generate_decode_input_arms(&functions, &type_map, &custom_handlers);

        assert_eq!(
            arms.len(),
            0,
            "EncodeInput handler should not appear in decode_input_arms"
        );
    }

    #[test]
    fn test_custom_handler_wrong_role_not_included_in_decode_output() {
        let type_map = HashMap::new();
        let functions = vec![];

        // DecodeInput handler should NOT appear in decode_output_arms
        let custom_handlers = vec![make_custom_handler(
            "extra_data",
            DataDriverRole::DecodeInput,
            "decode_extra_input",
        )];

        let arms = generate_decode_output_arms(&functions, &type_map, &custom_handlers);

        assert_eq!(
            arms.len(),
            0,
            "DecodeInput handler should not appear in decode_output_arms"
        );
    }

    #[test]
    fn test_encode_input_mixed_regular_and_custom() {
        let mut type_map = HashMap::new();
        type_map.insert("Address".to_string(), "my_crate::Address".to_string());

        let functions = vec![
            make_function("init", quote! { Address }, quote! { () }, false),
            make_function("is_paused", quote! { () }, quote! { bool }, false),
        ];

        let custom_handlers = vec![make_custom_handler(
            "extra_data",
            DataDriverRole::EncodeInput,
            "encode_extra_data",
        )];

        let arms = generate_encode_input_arms(&functions, &type_map, &custom_handlers);

        // Should have 2 regular functions + 1 custom handler
        assert_eq!(arms.len(), 3);

        let all_arms: String = arms.iter().map(|a| normalize_tokens(a.clone())).collect();

        // Verify regular functions use json_to_rkyv
        assert!(all_arms.contains("\"init\""));
        assert!(all_arms.contains("\"is_paused\""));
        assert!(all_arms.contains("json_to_rkyv"));

        // Verify custom handler calls the handler function
        assert!(all_arms.contains("\"extra_data\""));
        assert!(all_arms.contains("encode_extra_data (json)"));
    }

    #[test]
    fn test_decode_output_mixed_regular_and_custom() {
        let type_map = HashMap::new();

        let functions = vec![
            make_function("pause", quote! { () }, quote! { () }, false),
            make_function("get_value", quote! { () }, quote! { u64 }, false),
        ];

        let custom_handlers = vec![make_custom_handler(
            "extra_data",
            DataDriverRole::DecodeOutput,
            "decode_extra_output",
        )];

        let arms = generate_decode_output_arms(&functions, &type_map, &custom_handlers);

        // Should have 2 regular functions + 1 custom handler
        assert_eq!(arms.len(), 3);

        let all_arms: String = arms.iter().map(|a| normalize_tokens(a.clone())).collect();

        // Verify pause returns Null (unit type)
        assert!(all_arms.contains("\"pause\""));
        assert!(all_arms.contains("JsonValue :: Null"));

        // Verify get_value uses u64 special handler
        assert!(all_arms.contains("\"get_value\""));
        assert!(all_arms.contains("rkyv_to_json_u64"));

        // Verify custom handler calls the handler function
        assert!(all_arms.contains("\"extra_data\""));
        assert!(all_arms.contains("decode_extra_output (rkyv)"));
    }

    // =========================================================================
    // generate_decode_output_arms tests
    // =========================================================================

    #[test]
    fn test_decode_output_unit_returns_null() {
        let type_map = HashMap::new();

        let functions = vec![make_function("pause", quote! { () }, quote! { () }, false)];
        let arms = generate_decode_output_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"pause\""));
        assert!(arm_str.contains("Ok"));
        assert!(arm_str.contains("JsonValue :: Null"));
        // Verify it does NOT use rkyv_to_json (the generic version)
        assert!(
            !arm_str.contains("rkyv_to_json"),
            "Unit type should return Null directly, not use rkyv_to_json"
        );
    }

    #[test]
    fn test_decode_output_u64_uses_special_handler() {
        let type_map = HashMap::new();

        let functions = vec![make_function(
            "finalization_period",
            quote! { () },
            quote! { u64 },
            false,
        )];
        let arms = generate_decode_output_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"finalization_period\""));
        assert!(arm_str.contains("rkyv_to_json_u64"));
        // Verify it does NOT use the generic rkyv_to_json::<u64>
        assert!(
            !arm_str.contains("rkyv_to_json :: < u64 >"),
            "u64 should use rkyv_to_json_u64, not generic version"
        );
    }

    #[test]
    fn test_decode_output_bool() {
        let type_map = HashMap::new();

        let functions = vec![make_function(
            "is_paused",
            quote! { () },
            quote! { bool },
            false,
        )];
        let arms = generate_decode_output_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"is_paused\""));
        assert!(arm_str.contains("rkyv_to_json :: < bool >"));
        // Verify it does NOT use the special handlers for unit or u64
        assert!(!arm_str.contains("JsonValue :: Null"));
        assert!(!arm_str.contains("rkyv_to_json_u64"));
    }

    #[test]
    fn test_decode_output_complex_type() {
        let mut type_map = HashMap::new();
        type_map.insert(
            "Option < PendingItem >".to_string(),
            "Option<my_crate::PendingItem>".to_string(),
        );

        let functions = vec![make_function(
            "pending_withdrawal",
            quote! { ItemId },
            quote! { Option<PendingItem> },
            false,
        )];
        let arms = generate_decode_output_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"pending_withdrawal\""));
        assert!(arm_str.contains("rkyv_to_json"));
        // Verify the resolved type is used
        assert!(
            arm_str.contains("my_crate :: PendingItem"),
            "Should use resolved type: {}",
            arm_str
        );
    }

    #[test]
    fn test_decode_output_custom_returns_error() {
        let type_map = HashMap::new();

        let functions = vec![make_function(
            "custom_fn",
            quote! { () },
            quote! { Vec<u8> },
            true,
        )];
        let arms = generate_decode_output_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("Err"));
        assert!(
            arm_str.contains("\"decode_output\""),
            "error should name the decode_output role: {arm_str}"
        );
        assert!(
            arm_str.contains(&handler_signature_display(DataDriverRole::DecodeOutput)),
            "error should include the decode_output signature verbatim: {arm_str}"
        );
    }

    #[test]
    fn test_decode_output_with_custom_handler() {
        let type_map = HashMap::new();
        let functions = vec![];

        let custom_handlers = vec![make_custom_handler(
            "extra_data",
            DataDriverRole::DecodeOutput,
            "decode_extra_output",
        )];

        let arms = generate_decode_output_arms(&functions, &type_map, &custom_handlers);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"extra_data\""));
        assert!(arm_str.contains("decode_extra_output"));
        assert!(arm_str.contains("(rkyv)"));
    }

    // =========================================================================
    // feed_type tests (for functions using abi::feed)
    // =========================================================================

    /// Helper to create a FunctionInfo with a feed_type.
    fn make_function_with_feed(
        name: &str,
        input: TokenStream2,
        output: TokenStream2,
        feed: TokenStream2,
    ) -> FunctionInfo {
        FunctionInfo {
            name: format_ident!("{}", name),
            doc: None,
            params: vec![],
            input_type: input,
            output_type: output,
            is_custom: false,
            returns_ref: false,
            receiver: Receiver::Ref,
            trait_name: None,
            feed_type: Some(feed),
        }
    }

    #[test]
    fn test_decode_output_uses_feed_type_instead_of_output_type() {
        let mut type_map = HashMap::new();
        type_map.insert(
            "(ItemId , PendingItem)".to_string(),
            "(my_crate::ItemId, my_crate::PendingItem)".to_string(),
        );

        // Function returns () but feeds (ItemId, PendingItem)
        let functions = vec![make_function_with_feed(
            "pending_withdrawals",
            quote! { () },
            quote! { () },
            quote! { (ItemId, PendingItem) },
        )];
        let arms = generate_decode_output_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"pending_withdrawals\""));
        // Should use the feed type, not return JsonValue::Null
        assert!(
            !arm_str.contains("JsonValue :: Null"),
            "Should NOT return Null when feed_type is present: {}",
            arm_str
        );
        assert!(
            arm_str.contains("rkyv_to_json"),
            "Should use rkyv_to_json with feed type: {}",
            arm_str
        );
        assert!(
            arm_str.contains("my_crate :: ItemId"),
            "Should use resolved feed type: {}",
            arm_str
        );
    }

    #[test]
    fn test_decode_output_feed_type_simple() {
        let mut type_map = HashMap::new();
        type_map.insert("ItemId".to_string(), "my_crate::ItemId".to_string());

        // Function returns () but feeds ItemId
        let functions = vec![make_function_with_feed(
            "finalized_withdrawals",
            quote! { () },
            quote! { () },
            quote! { ItemId },
        )];
        let arms = generate_decode_output_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"finalized_withdrawals\""));
        assert!(
            arm_str.contains("my_crate :: ItemId"),
            "Should use resolved feed type: {}",
            arm_str
        );
    }

    #[test]
    fn test_decode_output_no_feed_type_uses_output_type() {
        let type_map = HashMap::new();

        // Function without feed_type should use output_type as before
        let functions = vec![make_function(
            "is_paused",
            quote! { () },
            quote! { bool },
            false,
        )];
        let arms = generate_decode_output_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("rkyv_to_json :: < bool >"));
    }

    #[test]
    fn test_decode_output_feed_type_u64_uses_special_handler() {
        let type_map = HashMap::new();

        // Function that feeds u64 should still use the special rkyv_to_json_u64
        let functions = vec![make_function_with_feed(
            "get_count",
            quote! { () },
            quote! { () },
            quote! { u64 },
        )];
        let arms = generate_decode_output_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"get_count\""));
        assert!(
            arm_str.contains("rkyv_to_json_u64"),
            "u64 feed_type should use rkyv_to_json_u64: {}",
            arm_str
        );
    }

    // =========================================================================
    // generate_decode_event_arms tests
    // =========================================================================

    #[test]
    fn test_decode_event_with_const_topic() {
        let mut type_map = HashMap::new();
        type_map.insert(
            "events::PauseToggled::PAUSED".to_string(),
            "my_crate::events::PauseToggled::PAUSED".to_string(),
        );
        type_map.insert(
            "events :: PauseToggled".to_string(),
            "my_crate::events::PauseToggled".to_string(),
        );

        let events = vec![make_event(
            "events::PauseToggled::PAUSED",
            quote! { events::PauseToggled },
        )];
        let arms = generate_decode_event_arms(&events, &type_map);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        // Verify topic is resolved
        assert!(
            arm_str.contains("my_crate :: events :: PauseToggled :: PAUSED"),
            "Topic should be resolved: {}",
            arm_str
        );
        // Verify data type is resolved
        assert!(
            arm_str.contains("rkyv_to_json :: < my_crate :: events :: PauseToggled >"),
            "Data type should be resolved: {}",
            arm_str
        );
    }

    #[test]
    fn test_decode_event_with_multi_segment_topic() {
        let type_map = HashMap::new();

        // Multi-segment paths are kept regardless of case
        let events = vec![make_event("events::Paused", quote! { PauseEvent })];
        let arms = generate_decode_event_arms(&events, &type_map);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("events :: Paused"));
        assert!(arm_str.contains("rkyv_to_json :: < PauseEvent >"));
    }

    #[test]
    fn test_decode_event_skips_lowercase_variable() {
        let type_map = HashMap::new();

        // Lowercase single identifier should be skipped (it's a variable reference)
        let events = vec![make_event("topic", quote! { SomeEvent })];
        let arms = generate_decode_event_arms(&events, &type_map);

        assert_eq!(arms.len(), 0, "Should skip lowercase variable reference");
    }

    #[test]
    fn test_decode_event_uppercase_single_ident_kept() {
        let type_map = HashMap::new();

        // Uppercase single identifier should be kept (it's a constant)
        let events = vec![make_event("PAUSED", quote! { PauseEvent })];
        let arms = generate_decode_event_arms(&events, &type_map);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("PAUSED"));
        // Verify the data type is also included
        assert!(
            arm_str.contains("rkyv_to_json :: < PauseEvent >"),
            "Should decode to PauseEvent type: {}",
            arm_str
        );
    }

    #[test]
    fn test_decode_event_string_literal_topic() {
        let type_map = HashMap::new();

        // A string literal topic that cannot be parsed as a syn::Path
        // (e.g., contains characters not valid in Rust paths)
        let events = vec![make_event("custom/event", quote! { TransferEvent })];
        let arms = generate_decode_event_arms(&events, &type_map);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        // String literal topics are used directly in the match arm
        assert!(
            arm_str.contains("\"custom/event\""),
            "Should use string literal topic: {}",
            arm_str
        );
        assert!(
            arm_str.contains("rkyv_to_json :: < TransferEvent >"),
            "Should decode to TransferEvent type: {}",
            arm_str
        );
    }

    #[test]
    fn test_decode_event_multiple_events() {
        let mut type_map = HashMap::new();
        type_map.insert(
            "events::PauseToggled::PAUSED".to_string(),
            "my_crate::events::PauseToggled::PAUSED".to_string(),
        );
        type_map.insert(
            "events::ItemAdded::TOPIC".to_string(),
            "my_crate::events::ItemAdded::TOPIC".to_string(),
        );

        let events = vec![
            make_event("events::PauseToggled::PAUSED", quote! { PauseToggled }),
            make_event("events::ItemAdded::TOPIC", quote! { ItemAdded }),
        ];
        let arms = generate_decode_event_arms(&events, &type_map);

        assert_eq!(arms.len(), 2);

        // Verify both events are present with correct resolved topics
        let all_arms: String = arms.iter().map(|a| normalize_tokens(a.clone())).collect();
        assert!(
            all_arms.contains("my_crate :: events :: PauseToggled :: PAUSED"),
            "Should contain resolved PauseToggled topic"
        );
        assert!(
            all_arms.contains("my_crate :: events :: ItemAdded :: TOPIC"),
            "Should contain resolved ItemAdded topic"
        );
        // Verify data types are present
        assert!(all_arms.contains("PauseToggled"));
        assert!(all_arms.contains("ItemAdded"));
    }

    // =========================================================================
    // Integration test for module generation
    // =========================================================================

    #[test]
    fn test_module_generates_complete_structure() {
        let mut type_map = HashMap::new();
        type_map.insert("Address".to_string(), "my_crate::Address".to_string());

        let functions = vec![
            make_function("init", quote! { Address }, quote! { () }, false),
            make_function("is_paused", quote! { () }, quote! { bool }, false),
        ];

        let events = vec![make_event("PAUSED", quote! { PauseEvent })];

        let output = module(&[], &type_map, &functions, &events, &[]);
        let output_str = normalize_tokens(output);

        // Verify module structure
        assert!(output_str.contains("pub mod data_driver"));
        assert!(output_str.contains("pub struct Driver"));
        assert!(output_str.contains("impl dusk_data_driver :: ConvertibleContract for Driver"));

        // Verify all trait methods are present
        assert!(output_str.contains("fn encode_input_fn"));
        assert!(output_str.contains("fn decode_input_fn"));
        assert!(output_str.contains("fn decode_output_fn"));
        assert!(output_str.contains("fn decode_event"));
        assert!(output_str.contains("fn get_schema"));

        // Verify function match arms
        assert!(output_str.contains("\"init\""));
        assert!(output_str.contains("\"is_paused\""));

        // Verify WASM entrypoint
        assert!(output_str.contains("generate_wasm_entrypoint"));
    }

    // =========================================================================
    // role_name / handler_signature_display tests
    // =========================================================================

    #[test]
    fn test_role_name_covers_each_role() {
        // Role names must match the attribute the user writes at the
        // handler's definition site — anything else would send users
        // looking for a `decode-input` attribute that doesn't exist.
        assert_eq!(role_name(DataDriverRole::EncodeInput), "encode_input");
        assert_eq!(role_name(DataDriverRole::DecodeInput), "decode_input");
        assert_eq!(role_name(DataDriverRole::DecodeOutput), "decode_output");
    }

    #[test]
    fn test_handler_signature_encode_input() {
        let sig = handler_signature(DataDriverRole::EncodeInput);
        assert_eq!(normalize_tokens(sig.arg_type.clone()), "& str");
        assert_eq!(
            normalize_tokens(sig.return_type.clone()),
            "Result < alloc :: vec :: Vec < u8 > , dusk_data_driver :: Error >"
        );
    }

    #[test]
    fn test_handler_signature_decode_input_matches_decode_output() {
        // Both decoder roles take the same rkyv bytes and return the same
        // JsonValue — the dispatch site uses identical call shapes, so the
        // canonical signatures must agree.
        let decode_input = handler_signature(DataDriverRole::DecodeInput);
        let decode_output = handler_signature(DataDriverRole::DecodeOutput);
        assert_eq!(
            normalize_tokens(decode_input.arg_type.clone()),
            normalize_tokens(decode_output.arg_type.clone()),
        );
        assert_eq!(
            normalize_tokens(decode_input.return_type.clone()),
            normalize_tokens(decode_output.return_type.clone()),
        );
        assert_eq!(normalize_tokens(decode_input.arg_type), "& [u8]");
        assert_eq!(
            normalize_tokens(decode_input.return_type),
            "Result < dusk_data_driver :: JsonValue , dusk_data_driver :: Error >"
        );
    }

    #[test]
    fn test_handler_signature_display_format() {
        // The display form is what diagnostics show the user — it must
        // render as a complete `fn(arg) -> ret` form they can copy-paste.
        assert_eq!(
            handler_signature_display(DataDriverRole::EncodeInput),
            "fn(&str) -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error>",
        );
        assert_eq!(
            handler_signature_display(DataDriverRole::DecodeInput),
            "fn(&[u8]) -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error>",
        );
        assert_eq!(
            handler_signature_display(DataDriverRole::DecodeOutput),
            "fn(&[u8]) -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error>",
        );
    }

    // =========================================================================
    // reemit_imports / collect_handler_identifiers tests
    // =========================================================================
    //
    // These back the splice-side half of the validator-vs-splicer contract:
    // the validator accepts short-path handlers only if the splicer can make
    // those same paths resolve in the generated submodule. `reemit_imports`
    // decides which user imports follow the handlers into the submodule;
    // `collect_handler_identifiers` feeds its filter. A bug here resurfaces
    // Defect 3 — a validator-only unit test won't catch it.

    fn import(name: &str, path: &str) -> ImportInfo {
        ImportInfo {
            name: name.into(),
            path: path.into(),
        }
    }

    fn handler(func: syn::ItemFn) -> CustomDataDriverHandler {
        CustomDataDriverHandler {
            fn_name: "h".into(),
            role: DataDriverRole::EncodeInput,
            func,
        }
    }

    fn emitted_as_string(stream: &TokenStream2) -> String {
        stream.to_string()
    }

    #[test]
    fn test_reemit_imports_skips_unreferenced() {
        // A handler that only mentions `Error` must not pull the `Unused`
        // import into the data-driver submodule — otherwise contract-only
        // imports (e.g. `types::Ownable` gated behind the `abi` feature)
        // would break the data-driver build.
        let imports = vec![
            import("Error", "foo::Error"),
            import("Unused", "bar::Unused"),
        ];
        let h = handler(syn::parse_quote! {
            fn h(x: &str) -> Result<(), Error> { unimplemented!() }
        });
        let emitted = reemit_imports(&imports, &[h]);

        assert_eq!(emitted.len(), 1, "only `Error` should be re-emitted");
        let s = emitted_as_string(&emitted[0]);
        assert!(s.contains("Error"), "emitted `use` references Error: {s}");
        assert!(!s.contains("Unused"), "Unused import must be filtered out");
    }

    #[test]
    fn test_reemit_imports_preserves_rename() {
        // `use foo::Bar as Baz;` is how the parser records a renamed import
        // (`name` = "Baz", `path` = "foo::Bar"). When the handler references
        // `Baz`, re-emit must produce `use foo::Bar as Baz;` — keeping the
        // original type reachable under the alias the handler uses.
        let imports = vec![import("Baz", "foo::Bar")];
        let h = handler(syn::parse_quote! {
            fn h(x: &str) -> Result<(), Baz> { unimplemented!() }
        });
        let emitted = reemit_imports(&imports, &[h]);

        assert_eq!(emitted.len(), 1);
        let s = emitted_as_string(&emitted[0]);
        assert!(s.contains("Bar"), "emit references the real path: {s}");
        assert!(s.contains("Baz"), "emit preserves the alias: {s}");
        assert!(s.contains("as"), "emit uses `as` for renamed imports: {s}");
    }

    #[test]
    fn test_reemit_imports_plain_path_omits_as() {
        // `use foo::Bar;` (no rename) must emit without an `as` clause — a
        // stray self-alias like `use foo::Bar as Bar;` is legal Rust but
        // noisy in expanded output and a signal the rename detection is
        // off.
        let imports = vec![import("Bar", "foo::Bar")];
        let h = handler(syn::parse_quote! {
            fn h(x: &str) -> Result<(), Bar> { unimplemented!() }
        });
        let emitted = reemit_imports(&imports, &[h]);

        assert_eq!(emitted.len(), 1);
        let s = emitted_as_string(&emitted[0]);
        assert!(
            !s.contains(" as "),
            "plain imports must not emit a self-rename: {s}"
        );
        assert!(s.contains("Bar"));
    }

    #[test]
    fn test_reemit_imports_no_handlers_emits_nothing() {
        // Without handlers, there's nothing to resolve short paths for —
        // re-emitting any import is wasted noise and risks unrelated
        // conflicts in the submodule.
        let imports = vec![import("Error", "foo::Error")];
        let emitted = reemit_imports(&imports, &[]);
        assert!(emitted.is_empty());
    }

    #[test]
    fn test_reemit_imports_multi_segment_path() {
        // `use dusk_data_driver::Error;` → 3-segment path. The emit must
        // carry the full path so the alias resolves to the right type, not
        // just `Error` (which wouldn't be in scope in the submodule).
        let imports = vec![import("Error", "dusk_data_driver::Error")];
        let h = handler(syn::parse_quote! {
            fn h(x: &str) -> Result<(), Error> { unimplemented!() }
        });
        let emitted = reemit_imports(&imports, &[h]);

        let s = emitted_as_string(&emitted[0]);
        assert!(
            s.contains("dusk_data_driver"),
            "emit carries the full path: {s}"
        );
        assert!(s.contains("Error"));
    }

    #[test]
    fn test_collect_handler_identifiers_from_signature() {
        // Signature references: `Result`, `Vec`, `u8`, `Error`. Arg type
        // `&str` is split into `&` (punct) + `str` (ident) — only `str`
        // counts. The collector must pick up signature idents even without
        // a body.
        let h = handler(syn::parse_quote! {
            fn h(json: &str) -> Result<Vec<u8>, Error> { unimplemented!() }
        });
        let idents = collect_handler_identifiers(&[h]);
        for name in ["Result", "Vec", "u8", "Error", "str", "json"] {
            assert!(
                idents.contains(name),
                "{name} should be collected from signature, got: {idents:?}"
            );
        }
    }

    #[test]
    fn test_collect_handler_identifiers_walks_nested_groups() {
        // Closures, blocks, and method chains nest tokens inside
        // `TokenTree::Group` — the walker must recurse into them or body
        // references like `.map_err(Error::from)` get missed and their
        // imports would be wrongly filtered out.
        let h = handler(syn::parse_quote! {
            fn h(b: &[u8]) -> Result<(), Error> {
                let _result = (|| Error::from(()))();
                Ok(())
            }
        });
        let idents = collect_handler_identifiers(&[h]);
        assert!(
            idents.contains("Error"),
            "identifier inside closure body must be collected: {idents:?}"
        );
        assert!(
            idents.contains("from"),
            "method path segments inside closures must be collected: {idents:?}"
        );
    }

    #[test]
    fn test_collect_handler_identifiers_empty() {
        // No handlers → empty set, not a panic. Guards the zero-handler
        // fast path that `reemit_imports` relies on to emit nothing.
        let idents = collect_handler_identifiers(&[]);
        assert!(idents.is_empty());
    }
}
