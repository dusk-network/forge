// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Code generation functions for the contract macro.

use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{ImplItem, ItemImpl};

use crate::{generate_arg_expr, EventInfo, FunctionInfo, ImportInfo, Receiver};

/// Generate the schema constant.
pub(crate) fn schema(
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
                dusk_forge::schema::Import {
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
                dusk_forge::schema::Function {
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
                dusk_forge::schema::Event {
                    topic: #topic,
                    data: #data_str,
                }
            }
        })
        .collect();

    quote! {
        /// Contract schema containing metadata about functions, events, and imports.
        pub const CONTRACT_SCHEMA: dusk_forge::schema::Contract = dusk_forge::schema::Contract {
            name: #contract_name_lit,
            imports: &[#(#import_entries),*],
            functions: &[#(#function_entries),*],
            events: &[#(#event_entries),*],
        };
    }
}

/// Generate the static `STATE` variable declaration.
///
/// This creates a mutable static variable initialized via the contract's `new()` constructor:
/// ```ignore
/// static mut STATE: ContractName = ContractName::new();
/// ```
pub(crate) fn state_static(contract_ident: &Ident) -> TokenStream2 {
    quote! {
        /// Static contract state initialized via `new()`.
        #[cfg(target_family = "wasm")]
        static mut STATE: #contract_ident = #contract_ident::new();
    }
}

/// Generate extern "C" wrapper functions for all public methods.
///
/// Each wrapper deserializes input, calls the method on STATE, and serializes output.
/// - For methods that return references, the wrapper clones the result before serialization.
/// - For parameters that are references, the wrapper receives the owned value and passes a reference.
/// - For trait methods with default implementations, calls the trait method via fully-qualified syntax.
/// - For associated functions (no self), calls the function on the contract type.
pub(crate) fn extern_wrappers(functions: &[FunctionInfo], contract_ident: &Ident) -> TokenStream2 {
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
                    let arg_expr = generate_arg_expr(param);
                    (quote! { #name: #ty }, arg_expr)
                }
                _ => {
                    // Multiple parameters: |(p1, p2, ...): (T1, T2, ...)|
                    let names: Vec<_> = f.params.iter().map(|p| &p.name).collect();
                    let arg_exprs: Vec<_> = f.params.iter().map(generate_arg_expr).collect();
                    (
                        quote! { (#(#names),*): #input_type },
                        quote! { #(#arg_exprs),* },
                    )
                }
            };

            // Generate the method call based on whether it's a regular method,
            // trait method, or associated function
            let has_receiver = f.receiver != Receiver::None;
            let method_call = match (&f.trait_name, has_receiver) {
                // Trait method with default impl (empty body) - call via trait
                (Some(trait_name), true) => {
                    let trait_ident = format_ident!("{}", trait_name);
                    let state_ref = if f.receiver == Receiver::RefMut {
                        quote! { &mut STATE }
                    } else {
                        quote! { &STATE }
                    };
                    if f.returns_ref {
                        quote! { #trait_ident::#fn_name(#state_ref, #method_args).clone() }
                    } else {
                        quote! { #trait_ident::#fn_name(#state_ref, #method_args) }
                    }
                }
                // Trait associated function with default impl (no self)
                (Some(trait_name), false) => {
                    let trait_ident = format_ident!("{}", trait_name);
                    if f.returns_ref {
                        quote! { <#contract_ident as #trait_ident>::#fn_name(#method_args).clone() }
                    } else {
                        quote! { <#contract_ident as #trait_ident>::#fn_name(#method_args) }
                    }
                }
                // Regular method - call on STATE
                (None, true) => {
                    if f.returns_ref {
                        quote! { STATE.#fn_name(#method_args).clone() }
                    } else {
                        quote! { STATE.#fn_name(#method_args) }
                    }
                }
                // Associated function (no self, no trait) - shouldn't happen but handle it
                (None, false) => {
                    if f.returns_ref {
                        quote! { #contract_ident::#fn_name(#method_args).clone() }
                    } else {
                        quote! { #contract_ident::#fn_name(#method_args) }
                    }
                }
            };

            quote! {
                #[no_mangle]
                unsafe extern "C" fn #fn_name(arg_len: u32) -> u32 {
                    dusk_core::abi::wrap_call(arg_len, |#closure_param| #method_call)
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

/// Strip #[contract(...)] attributes from the impl block and its methods.
/// For trait impl blocks, also removes empty-body methods (they're just signature stubs
/// for wrapper generation and should use the trait's default implementation).
pub(crate) fn strip_contract_attributes(mut impl_block: ItemImpl) -> ItemImpl {
    let is_trait_impl = impl_block.trait_.is_some();

    // Strip from the impl block itself (e.g., #[contract(expose = [...])])
    impl_block
        .attrs
        .retain(|attr| !attr.path().is_ident("contract"));

    // Strip from methods (e.g., #[contract(custom)])
    for item in &mut impl_block.items {
        if let ImplItem::Fn(method) = item {
            method
                .attrs
                .retain(|attr| !attr.path().is_ident("contract"));
        }
    }

    // For trait impls, remove empty-body methods so they use the default implementation
    if is_trait_impl {
        impl_block.items.retain(|item| {
            if let ImplItem::Fn(method) = item {
                !method.block.stmts.is_empty()
            } else {
                true
            }
        });
    }

    impl_block
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ParameterInfo, Receiver};

    fn normalize_tokens(tokens: TokenStream2) -> String {
        tokens
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn test_extern_wrapper_no_params() {
        let contract_ident = format_ident!("MyContract");
        let functions = vec![FunctionInfo {
            name: format_ident!("is_paused"),
            doc: Some("Returns pause state.".to_string()),
            params: vec![],
            input_type: quote! { () },
            output_type: quote! { bool },
            is_custom: false,
            returns_ref: false,
            receiver: Receiver::Ref,
            trait_name: None,
            feed_type: None,
        }];

        let output = normalize_tokens(extern_wrappers(&functions, &contract_ident));

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
        let contract_ident = format_ident!("MyContract");
        let functions = vec![FunctionInfo {
            name: format_ident!("init"),
            doc: Some("Initialize.".to_string()),
            params: vec![ParameterInfo {
                name: format_ident!("owner"),
                ty: quote! { Address },
                is_ref: false,
                is_mut_ref: false,
            }],
            input_type: quote! { Address },
            output_type: quote! { () },
            is_custom: false,
            returns_ref: false,
            receiver: Receiver::RefMut,
            trait_name: None,
            feed_type: None,
        }];

        let output = normalize_tokens(extern_wrappers(&functions, &contract_ident));

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
        let contract_ident = format_ident!("MyContract");
        let functions = vec![FunctionInfo {
            name: format_ident!("transfer"),
            doc: Some("Transfer funds.".to_string()),
            params: vec![
                ParameterInfo {
                    name: format_ident!("to"),
                    ty: quote! { Address },
                    is_ref: false,
                    is_mut_ref: false,
                },
                ParameterInfo {
                    name: format_ident!("amount"),
                    ty: quote! { u64 },
                    is_ref: false,
                    is_mut_ref: false,
                },
            ],
            input_type: quote! { (Address, u64) },
            output_type: quote! { () },
            is_custom: false,
            returns_ref: false,
            receiver: Receiver::RefMut,
            trait_name: None,
            feed_type: None,
        }];

        let output = normalize_tokens(extern_wrappers(&functions, &contract_ident));

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
        let contract_ident = format_ident!("MyContract");
        let functions = vec![
            FunctionInfo {
                name: format_ident!("pause"),
                doc: None,
                params: vec![],
                input_type: quote! { () },
                output_type: quote! { () },
                is_custom: false,
                returns_ref: false,
                receiver: Receiver::RefMut,
                trait_name: None,
                feed_type: None,
            },
            FunctionInfo {
                name: format_ident!("unpause"),
                doc: None,
                params: vec![],
                input_type: quote! { () },
                output_type: quote! { () },
                is_custom: false,
                returns_ref: false,
                receiver: Receiver::RefMut,
                trait_name: None,
                feed_type: None,
            },
        ];

        let output = normalize_tokens(extern_wrappers(&functions, &contract_ident));

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
    fn test_extern_wrapper_returns_ref() {
        let contract_ident = format_ident!("MyContract");
        let functions = vec![FunctionInfo {
            name: format_ident!("get_data"),
            doc: None,
            params: vec![],
            input_type: quote! { () },
            output_type: quote! { LargeStruct },
            is_custom: false,
            returns_ref: true,
            receiver: Receiver::Ref,
            trait_name: None,
            feed_type: None,
        }];

        let output = normalize_tokens(extern_wrappers(&functions, &contract_ident));

        let expected = normalize_tokens(quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[no_mangle]
                unsafe extern "C" fn get_data(arg_len: u32) -> u32 {
                    dusk_core::abi::wrap_call(arg_len, |(): ()| STATE.get_data().clone())
                }
            }
        });

        assert_eq!(expected, output);
    }

    #[test]
    fn test_extern_wrapper_ref_param() {
        let contract_ident = format_ident!("MyContract");
        let functions = vec![FunctionInfo {
            name: format_ident!("process"),
            doc: None,
            params: vec![ParameterInfo {
                name: format_ident!("data"),
                ty: quote! { LargeStruct },
                is_ref: true,
                is_mut_ref: false,
            }],
            input_type: quote! { LargeStruct },
            output_type: quote! { () },
            is_custom: false,
            returns_ref: false,
            receiver: Receiver::RefMut,
            trait_name: None,
            feed_type: None,
        }];

        let output = normalize_tokens(extern_wrappers(&functions, &contract_ident));

        let expected = normalize_tokens(quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[no_mangle]
                unsafe extern "C" fn process(arg_len: u32) -> u32 {
                    dusk_core::abi::wrap_call(arg_len, |data: LargeStruct| STATE.process(&data))
                }
            }
        });

        assert_eq!(expected, output);
    }

    #[test]
    fn test_extern_wrapper_mut_ref_param() {
        let contract_ident = format_ident!("MyContract");
        let functions = vec![FunctionInfo {
            name: format_ident!("modify"),
            doc: None,
            params: vec![ParameterInfo {
                name: format_ident!("data"),
                ty: quote! { Data },
                is_ref: true,
                is_mut_ref: true,
            }],
            input_type: quote! { Data },
            output_type: quote! { () },
            is_custom: false,
            returns_ref: false,
            receiver: Receiver::RefMut,
            trait_name: None,
            feed_type: None,
        }];

        let output = normalize_tokens(extern_wrappers(&functions, &contract_ident));

        let expected = normalize_tokens(quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[no_mangle]
                unsafe extern "C" fn modify(arg_len: u32) -> u32 {
                    dusk_core::abi::wrap_call(arg_len, |data: Data| STATE.modify(&mut data))
                }
            }
        });

        assert_eq!(expected, output);
    }

    #[test]
    fn test_state_static() {
        let contract_ident = format_ident!("MyContract");
        let output = normalize_tokens(state_static(&contract_ident));

        let expected = normalize_tokens(quote! {
            /// Static contract state initialized via `new()`.
            #[cfg(target_family = "wasm")]
            static mut STATE: MyContract = MyContract::new();
        });

        assert_eq!(expected, output);
    }
}
