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

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::resolve::TypeMap;
use crate::{EventInfo, FunctionInfo};

/// Generate the `data_driver` module at crate root level.
pub(crate) fn module(
    type_map: &TypeMap,
    functions: &[FunctionInfo],
    events: &[EventInfo],
) -> TokenStream2 {
    let encode_input_arms = generate_encode_input_arms(functions, type_map);
    let decode_input_arms = generate_decode_input_arms(functions, type_map);
    let decode_output_arms = generate_decode_output_arms(functions, type_map);
    let decode_event_arms = generate_decode_event_arms(events, type_map);

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
fn generate_encode_input_arms(functions: &[FunctionInfo], type_map: &TypeMap) -> Vec<TokenStream2> {
    functions
        .iter()
        .map(|f| {
            let name_str = f.name.to_string();
            let input_type = get_resolved_type(&f.input_type, type_map);
            quote! {
                #name_str => dusk_data_driver::json_to_rkyv::<#input_type>(json)
            }
        })
        .collect()
}

/// Generate match arms for `decode_input_fn`.
fn generate_decode_input_arms(functions: &[FunctionInfo], type_map: &TypeMap) -> Vec<TokenStream2> {
    functions
        .iter()
        .map(|f| {
            let name_str = f.name.to_string();
            let input_type = get_resolved_type(&f.input_type, type_map);
            quote! {
                #name_str => dusk_data_driver::rkyv_to_json::<#input_type>(rkyv)
            }
        })
        .collect()
}

/// Generate match arms for `decode_output_fn`.
///
/// When a function has a `feed_type` (from `#[contract(feeds = "Type")]`),
/// that type is used for decoding instead of the return type. This handles
/// functions that stream data via `abi::feed()` rather than returning directly.
fn generate_decode_output_arms(
    functions: &[FunctionInfo],
    type_map: &TypeMap,
) -> Vec<TokenStream2> {
    functions
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

            if type_str == "()" {
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
        .collect()
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
    fn make_function(name: &str, input: TokenStream2, output: TokenStream2) -> FunctionInfo {
        FunctionInfo {
            name: format_ident!("{}", name),
            doc: None,
            params: vec![],
            input_type: input,
            output_type: output,
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

        let functions = vec![make_function("init", quote! { Address }, quote! { () })];
        let arms = generate_encode_input_arms(&functions, &type_map);

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

        let functions = vec![make_function("is_paused", quote! { () }, quote! { bool })];
        let arms = generate_encode_input_arms(&functions, &type_map);

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
        )];
        let arms = generate_encode_input_arms(&functions, &type_map);

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
    fn test_encode_input_multiple_functions() {
        let type_map = HashMap::new();

        let functions = vec![
            make_function("pause", quote! { () }, quote! { () }),
            make_function("unpause", quote! { () }, quote! { () }),
            make_function("init", quote! { Address }, quote! { () }),
        ];
        let arms = generate_encode_input_arms(&functions, &type_map);

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

        let functions = vec![make_function("deposit", quote! { Deposit }, quote! { () })];
        let arms = generate_decode_input_arms(&functions, &type_map);

        assert_eq!(arms.len(), 1);
        let arm_str = normalize_tokens(arms[0].clone());
        assert!(arm_str.contains("\"deposit\""));
        assert!(arm_str.contains("rkyv_to_json"));
        assert!(arm_str.contains("my_crate :: Deposit"));
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
        )];
        let arms = generate_decode_input_arms(&functions, &type_map);

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

    // =========================================================================
    // generate_decode_output_arms tests
    // =========================================================================

    #[test]
    fn test_decode_output_unit_returns_null() {
        let type_map = HashMap::new();

        let functions = vec![make_function("pause", quote! { () }, quote! { () })];
        let arms = generate_decode_output_arms(&functions, &type_map);

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
        )];
        let arms = generate_decode_output_arms(&functions, &type_map);

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

        let functions = vec![make_function("is_paused", quote! { () }, quote! { bool })];
        let arms = generate_decode_output_arms(&functions, &type_map);

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
        )];
        let arms = generate_decode_output_arms(&functions, &type_map);

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
        let arms = generate_decode_output_arms(&functions, &type_map);

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
        let arms = generate_decode_output_arms(&functions, &type_map);

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
        let functions = vec![make_function("is_paused", quote! { () }, quote! { bool })];
        let arms = generate_decode_output_arms(&functions, &type_map);

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
        let arms = generate_decode_output_arms(&functions, &type_map);

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
            make_function("init", quote! { Address }, quote! { () }),
            make_function("is_paused", quote! { () }, quote! { bool }),
        ];

        let events = vec![make_event("PAUSED", quote! { PauseEvent })];

        let output = module(&type_map, &functions, &events);
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
