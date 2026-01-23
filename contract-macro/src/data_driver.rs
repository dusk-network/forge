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

/// Get the resolved type path from the `type_map`, or return the original if not found.
fn get_resolved_type(ty: &TokenStream2, type_map: &TypeMap) -> TokenStream2 {
    let key = ty.to_string();
    if let Some(resolved) = type_map.get(&key) {
        // Parse the resolved path string back into tokens
        if let Ok(path) = syn::parse_str::<syn::Path>(resolved) {
            return quote! { #path };
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
        .collect()
}

/// Generate match arms for `decode_input_fn`.
fn generate_decode_input_arms(functions: &[FunctionInfo], type_map: &TypeMap) -> Vec<TokenStream2> {
    functions
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
        .collect()
}

/// Generate match arms for `decode_output_fn`.
fn generate_decode_output_arms(
    functions: &[FunctionInfo],
    type_map: &TypeMap,
) -> Vec<TokenStream2> {
    functions
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
