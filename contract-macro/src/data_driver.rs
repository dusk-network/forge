// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Data driver module generation.
//!
//! Generates a `data_driver` module at crate root level containing a `Driver`
//! struct that implements the `ConvertibleContract` trait from `dusk-data-driver`.
//!
//! The module is feature-gated with `#[cfg(feature = "data-driver")]` and uses
//! fully-qualified type paths resolved at extraction time.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::resolve::TypeMap;
use crate::{CustomDataDriverHandler, DataDriverRole, EventInfo, FunctionInfo};

/// Generate the `data_driver` module at crate root level.
pub(crate) fn module(
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

    quote! {
        /// Auto-generated data driver module.
        ///
        /// This module provides a `Driver` struct implementing `ConvertibleContract`
        /// for encoding/decoding contract function inputs, outputs, and events.
        #[cfg(feature = "data-driver")]
        pub mod data_driver {
            extern crate alloc;
            use alloc::format;
            use alloc::string::String;
            use alloc::vec::Vec;

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
                ) -> Result<Vec<u8>, dusk_data_driver::Error> {
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

                fn get_schema(&self) -> String {
                    super::CONTRACT_SCHEMA.to_json()
                }
            }

            // WASM entrypoint for the data-driver
            #[cfg(target_family = "wasm")]
            dusk_data_driver::generate_wasm_entrypoint!(Driver);
        }
    }
}

/// Get the resolved type path from the `type_map`, or return the original if not found.
fn get_resolved_type(ty: &TokenStream2, type_map: &TypeMap) -> TokenStream2 {
    let key = ty.to_string();
    if let Some(resolved) = type_map.get(&key) {
        // Parse the resolved string back into tokens as a Type (not Path, since tuples aren't paths)
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
                quote! {
                    #name_str => Err(dusk_data_driver::Error::Unsupported(
                        alloc::format!("custom handler required: {}", #name_str)
                    ))
                }
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
                quote! {
                    #name_str => Err(dusk_data_driver::Error::Unsupported(
                        alloc::format!("custom handler required: {}", #name_str)
                    ))
                }
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
fn generate_decode_output_arms(
    functions: &[FunctionInfo],
    type_map: &TypeMap,
    custom_handlers: &[CustomDataDriverHandler],
) -> Vec<TokenStream2> {
    let mut arms: Vec<TokenStream2> = functions
        .iter()
        .map(|f| {
            let name_str = f.name.to_string();
            let output_type = get_resolved_type(&f.output_type, type_map);
            let output_str = f.output_type.to_string();

            if f.is_custom {
                quote! {
                    #name_str => Err(dusk_data_driver::Error::Unsupported(
                        alloc::format!("custom handler required: {}", #name_str)
                    ))
                }
            } else if output_str == "()" {
                quote! {
                    #name_str => Ok(dusk_data_driver::JsonValue::Null)
                }
            } else if output_str == "u64" {
                quote! {
                    #name_str => dusk_data_driver::rkyv_to_json_u64(rkyv)
                }
            } else {
                quote! {
                    #name_str => dusk_data_driver::rkyv_to_json::<#output_type>(rkyv)
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
            let resolved_topic = type_map.get(topic_str).map_or(topic_str.clone(), Clone::clone);

            // Try to parse the resolved topic as a path for constant resolution
            if let Ok(topic_path) = syn::parse_str::<syn::Path>(&resolved_topic) {
                // Skip variable references (single lowercase identifier)
                if topic_path.segments.len() == 1 {
                    let name = topic_path.segments[0].ident.to_string();
                    if name.chars().next().map_or(false, char::is_lowercase) {
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
    use super::*;
    use crate::Receiver;
    use quote::format_ident;
    use std::collections::HashMap;

    /// Normalize token stream to a string with consistent whitespace for comparison.
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
        type_map.insert("Address".to_string(), "evm_core::Address".to_string());

        let ty = quote! { Address };
        let resolved = get_resolved_type(&ty, &type_map);

        assert_eq!(normalize_tokens(resolved), "evm_core :: Address");
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
        type_map.insert(
            "Deposit".to_string(),
            "evm_core::standard_bridge::Deposit".to_string(),
        );

        let ty = quote! { Deposit };
        let resolved = get_resolved_type(&ty, &type_map);

        assert_eq!(
            normalize_tokens(resolved),
            "evm_core :: standard_bridge :: Deposit"
        );
    }

    // =========================================================================
    // generate_encode_input_arms tests
    // =========================================================================

    #[test]
    fn test_encode_input_simple_type() {
        let mut type_map = HashMap::new();
        type_map.insert("Address".to_string(), "evm_core::Address".to_string());

        let functions = vec![make_function("init", quote! { Address }, quote! { () }, false)];
        let arms = generate_encode_input_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"init\""), "Should contain function name");
        assert!(arm_str.contains("json_to_rkyv"), "Should use json_to_rkyv");
        assert!(
            arm_str.contains("evm_core :: Address"),
            "Should use resolved type"
        );
    }

    #[test]
    fn test_encode_input_unit_type() {
        let type_map = HashMap::new();

        let functions = vec![make_function("is_paused", quote! { () }, quote! { bool }, false)];
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
            "(evm_core::Address, u64)".to_string(),
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
            arm_str.contains("evm_core :: Address"),
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
        assert!(arm_str.contains("custom handler required"));
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
        type_map.insert("Deposit".to_string(), "evm_core::Deposit".to_string());

        let functions = vec![make_function("deposit", quote! { Deposit }, quote! { () }, false)];
        let arms = generate_decode_input_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"deposit\""));
        assert!(arm_str.contains("rkyv_to_json"));
        assert!(arm_str.contains("evm_core :: Deposit"));
    }

    #[test]
    fn test_decode_input_custom_returns_error() {
        let type_map = HashMap::new();

        let functions = vec![make_function("custom_fn", quote! { CustomType }, quote! { () }, true)];
        let arms = generate_decode_input_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("Err"));
        assert!(arm_str.contains("custom handler required"));
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
            "(Address , EVMAddress , u64)".to_string(),
            "(evm_core::Address, evm_core::EVMAddress, u64)".to_string(),
        );

        let functions = vec![make_function(
            "transfer_with_fee",
            quote! { (Address, EVMAddress, u64) },
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
            arm_str.contains("evm_core :: Address"),
            "Should use resolved Address type in tuple: {}",
            arm_str
        );
        assert!(
            arm_str.contains("evm_core :: EVMAddress"),
            "Should use resolved EVMAddress type in tuple: {}",
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
        type_map.insert("Address".to_string(), "evm_core::Address".to_string());

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

        let functions = vec![make_function("is_paused", quote! { () }, quote! { bool }, false)];
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
            "Option < PendingWithdrawal >".to_string(),
            "Option<evm_core::PendingWithdrawal>".to_string(),
        );

        let functions = vec![make_function(
            "pending_withdrawal",
            quote! { WithdrawalId },
            quote! { Option<PendingWithdrawal> },
            false,
        )];
        let arms = generate_decode_output_arms(&functions, &type_map, &[]);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"pending_withdrawal\""));
        assert!(arm_str.contains("rkyv_to_json"));
        // Verify the resolved type is used
        assert!(
            arm_str.contains("evm_core :: PendingWithdrawal"),
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
        assert!(arm_str.contains("custom handler required"));
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
    // generate_decode_event_arms tests
    // =========================================================================

    #[test]
    fn test_decode_event_with_const_topic() {
        let mut type_map = HashMap::new();
        type_map.insert(
            "events::PauseToggled::PAUSED".to_string(),
            "evm_core::events::PauseToggled::PAUSED".to_string(),
        );
        type_map.insert(
            "events :: PauseToggled".to_string(),
            "evm_core::events::PauseToggled".to_string(),
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
            arm_str.contains("evm_core :: events :: PauseToggled :: PAUSED"),
            "Topic should be resolved: {}",
            arm_str
        );
        // Verify data type is resolved
        assert!(
            arm_str.contains("rkyv_to_json :: < evm_core :: events :: PauseToggled >"),
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
        let events = vec![make_event("bridge/deposited", quote! { DepositEvent })];
        let arms = generate_decode_event_arms(&events, &type_map);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        // String literal topics are used directly in the match arm
        assert!(
            arm_str.contains("\"bridge/deposited\""),
            "Should use string literal topic: {}",
            arm_str
        );
        assert!(
            arm_str.contains("rkyv_to_json :: < DepositEvent >"),
            "Should decode to DepositEvent type: {}",
            arm_str
        );
    }

    #[test]
    fn test_decode_event_multiple_events() {
        let mut type_map = HashMap::new();
        type_map.insert(
            "events::PauseToggled::PAUSED".to_string(),
            "evm_core::events::PauseToggled::PAUSED".to_string(),
        );
        type_map.insert(
            "events::BridgeInitiated::TOPIC".to_string(),
            "evm_core::events::BridgeInitiated::TOPIC".to_string(),
        );

        let events = vec![
            make_event("events::PauseToggled::PAUSED", quote! { PauseToggled }),
            make_event("events::BridgeInitiated::TOPIC", quote! { BridgeInitiated }),
        ];
        let arms = generate_decode_event_arms(&events, &type_map);

        assert_eq!(arms.len(), 2);

        // Verify both events are present with correct resolved topics
        let all_arms: String = arms.iter().map(|a| normalize_tokens(a.clone())).collect();
        assert!(
            all_arms.contains("evm_core :: events :: PauseToggled :: PAUSED"),
            "Should contain resolved PauseToggled topic"
        );
        assert!(
            all_arms.contains("evm_core :: events :: BridgeInitiated :: TOPIC"),
            "Should contain resolved BridgeInitiated topic"
        );
        // Verify data types are present
        assert!(all_arms.contains("PauseToggled"));
        assert!(all_arms.contains("BridgeInitiated"));
    }

    // =========================================================================
    // Integration test for module generation
    // =========================================================================

    #[test]
    fn test_module_generates_complete_structure() {
        let mut type_map = HashMap::new();
        type_map.insert("Address".to_string(), "evm_core::Address".to_string());

        let functions = vec![
            make_function("init", quote! { Address }, quote! { () }, false),
            make_function("is_paused", quote! { () }, quote! { bool }, false),
        ];

        let events = vec![make_event("PAUSED", quote! { PauseEvent })];

        let output = module(&type_map, &functions, &events, &[]);
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
}
