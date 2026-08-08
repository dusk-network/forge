// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Code generation functions for the contract macro.

use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{ImplItem, Item, ItemImpl, ItemMod, Type};

use crate::parse::{Analysis, EventInfo, FunctionInfo, ImportInfo, ParameterInfo, Receiver};
use crate::{data_driver, resolve};

/// Assemble the full proc-macro output: the schema, the contract module
/// (with `#[contract(...)]` attributes stripped), and the data-driver module.
///
/// This is the single entry point `lib.rs` calls after `parse::analyze`.
pub(crate) fn contract_module(
    module: &ItemMod,
    items: &[Item],
    analysis: &Analysis,
) -> TokenStream2 {
    let Analysis {
        contract_ident,
        contract_name,
        imports,
        functions,
        events,
    } = analysis;

    let mod_vis = &module.vis;
    let mod_name = &module.ident;
    let mod_attrs = &module.attrs;

    // Module-local items are not `use` imports; feed them into the type map
    // (as `super::mod::NAME`, with `{ }` on consts) without adding schema entries.
    let mut all_imports = imports.clone();
    all_imports.extend(local_const_imports(items, mod_name));

    let type_map = resolve::build_type_map(&all_imports, functions, events);

    let schema = schema(contract_name, imports, functions, events, &type_map);
    let state_static = state_static(contract_ident);
    let externs = extern_wrappers(functions, contract_ident);

    let data_driver = data_driver::module(&type_map, functions, events);

    let stripped_items = stripped_module_items(items, contract_name);
    let data_visible_items = data_visible_items(items);

    quote! {
        #[cfg(not(any(feature = "contract", feature = "data-driver")))]
        compile_error!("Enable either 'contract' or 'data-driver' feature for WASM builds");

        #[cfg(all(feature = "contract", feature = "data-driver"))]
        compile_error!("Features 'contract' and 'data-driver' are mutually exclusive");

        #[cfg(any(feature = "contract", feature = "data-driver"))]
        #schema

        #[cfg(not(feature = "data-driver"))]
        #(#mod_attrs)*
        #mod_vis mod #mod_name {
            #(#stripped_items)*

            #state_static

            #externs
        }

        // Plain data (const/struct/enum/type + needed uses) for data-driver sibling codegen.
        #[cfg(feature = "data-driver")]
        #(#mod_attrs)*
        #mod_vis mod #mod_name {
            #(#data_visible_items)*
        }

        #data_driver
    }
}

/// Module-local plain data as implicit imports for [`resolve::build_type_map`].
/// Const paths are brace-wrapped so they splice correctly into const-generic positions.
fn local_const_imports(items: &[Item], mod_name: &Ident) -> Vec<ImportInfo> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Const(item_const) => Some(ImportInfo {
                name: item_const.ident.to_string(),
                path: format!("{{ super::{mod_name}::{} }}", item_const.ident),
            }),
            Item::Struct(item_struct) => Some(ImportInfo {
                name: item_struct.ident.to_string(),
                path: format!("super::{mod_name}::{}", item_struct.ident),
            }),
            Item::Enum(item_enum) => Some(ImportInfo {
                name: item_enum.ident.to_string(),
                path: format!("super::{mod_name}::{}", item_enum.ident),
            }),
            Item::Type(item_type) => Some(ImportInfo {
                name: item_type.ident.to_string(),
                path: format!("super::{mod_name}::{}", item_type.ident),
            }),
            _ => None,
        })
        .collect()
}

/// Plain-data module items safe under `data-driver` (no impl blocks / ABI-only uses).
fn data_visible_items(items: &[Item]) -> Vec<Item> {
    let plain: Vec<Item> = items
        .iter()
        .filter(|item| {
            matches!(
                item,
                Item::Const(_) | Item::Static(_) | Item::Struct(_) | Item::Enum(_) | Item::Type(_)
            )
        })
        .map(data_visible_item)
        .collect();

    let referenced: std::collections::HashSet<String> = plain
        .iter()
        .flat_map(item_type_words)
        .collect();

    let used_imports = items.iter().filter(|item| {
        let Item::Use(item_use) = item else {
            return false;
        };
        use_leaf_names(item_use)
            .iter()
            .any(|name| referenced.contains(name))
    });

    plain.into_iter().chain(used_imports.cloned()).collect()
}

/// Clone a plain-data item; `const`/`static` become `pub(crate)` so sibling `data_driver` can name them.
fn data_visible_item(item: &Item) -> Item {
    match item {
        Item::Const(item_const) => {
            let mut item_const = item_const.clone();
            item_const.vis = syn::parse_quote!(pub(crate));
            Item::Const(item_const)
        }
        Item::Static(item_static) => {
            let mut item_static = item_static.clone();
            item_static.vis = syn::parse_quote!(pub(crate));
            Item::Static(item_static)
        }
        Item::Type(item_type) => {
            let mut item_type = item_type.clone();
            item_type.vis = syn::parse_quote!(pub(crate));
            Item::Type(item_type)
        }
        _ => item.clone(),
    }
}

/// Type/expression identifier words in a plain-data item (not field/item names).
fn item_type_words(item: &Item) -> Vec<String> {
    let mut words = Vec::new();
    match item {
        Item::Struct(s) => {
            for field in &s.fields {
                let ty = &field.ty;
                words.extend(token_words(&quote! { #ty }.to_string()));
            }
        }
        Item::Enum(e) => {
            for variant in &e.variants {
                for field in &variant.fields {
                    let ty = &field.ty;
                    words.extend(token_words(&quote! { #ty }.to_string()));
                }
                if let Some((_, discriminant)) = &variant.discriminant {
                    words.extend(token_words(&quote! { #discriminant }.to_string()));
                }
            }
        }
        Item::Const(c) => {
            let (ty, expr) = (&c.ty, &c.expr);
            words.extend(token_words(&quote! { #ty }.to_string()));
            words.extend(token_words(&quote! { #expr }.to_string()));
        }
        Item::Static(s) => {
            let (ty, expr) = (&s.ty, &s.expr);
            words.extend(token_words(&quote! { #ty }.to_string()));
            words.extend(token_words(&quote! { #expr }.to_string()));
        }
        Item::Type(t) => {
            let ty = &t.ty;
            words.extend(token_words(&quote! { #ty }.to_string()));
        }
        _ => {}
    }
    words
}

fn token_words(s: &str) -> std::collections::HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn use_leaf_names(item_use: &syn::ItemUse) -> Vec<String> {
    fn walk(tree: &syn::UseTree, out: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(p) => walk(&p.tree, out),
            syn::UseTree::Name(n) => out.push(n.ident.to_string()),
            syn::UseTree::Rename(r) => out.push(r.rename.to_string()),
            syn::UseTree::Glob(_) => {}
            syn::UseTree::Group(g) => {
                for i in &g.items {
                    walk(i, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&item_use.tree, &mut out);
    out
}

/// Clone the module items, replacing every inherent or trait impl block for
/// the contract struct with a copy that has `#[contract(...)]` attributes
/// stripped (see [`strip_contract_attributes`]).
fn stripped_module_items(items: &[Item], contract_name: &str) -> Vec<Item> {
    items
        .iter()
        .map(|item| {
            if let Item::Impl(impl_block) = item
                && let Type::Path(type_path) = &*impl_block.self_ty
                && type_path.path.is_ident(contract_name)
            {
                Item::Impl(strip_contract_attributes(impl_block.clone()))
            } else {
                item.clone()
            }
        })
        .collect()
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

/// Generate the schema constant.
pub(crate) fn schema(
    contract_name: &str,
    imports: &[ImportInfo],
    functions: &[FunctionInfo],
    events: &[EventInfo],
    type_map: &resolve::TypeMap,
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

            // Convert type tokens to string for the schema
            let input_str = input.to_string();
            let output_str = output.to_string();

            quote! {
                dusk_forge::schema::Function {
                    name: #name_str,
                    doc: #doc,
                    input: #input_str,
                    output: #output_str,
                }
            }
        })
        .collect();

    let event_entries: Vec<_> = events
        .iter()
        .map(|e| {
            // The schema records the event type as written; topics are read
            // from its `ContractEvent` impl via the fully-resolved path so the
            // const is nameable at the schema's (module-parent) scope.
            let data_str = e.data_type.to_string();
            let resolved = resolve::resolved_tokens(&e.data_type, type_map);

            quote! {
                dusk_forge::schema::Event {
                    topics: <#resolved as dusk_forge::ContractEvent>::TOPICS,
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
/// This creates a mutable static variable initialized via the contract's
/// `new()` constructor:
///
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
/// Each wrapper deserializes input, calls the method on STATE, and serializes
/// output.
/// - For methods that return references, the wrapper clones the result before
///   serialization.
/// - For parameters that are references, the wrapper receives the owned value
///   and passes a reference.
/// - For trait methods with default implementations, calls the trait method via
///   fully-qualified syntax.
/// - For associated functions (no self), calls the function on the contract
///   type.
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
                #[unsafe(no_mangle)]
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
/// For trait impl blocks, also removes empty-body methods (they're just
/// signature stubs for wrapper generation and should use the trait's default
/// implementation).
pub(crate) fn strip_contract_attributes(mut impl_block: ItemImpl) -> ItemImpl {
    let is_trait_impl = impl_block.trait_.is_some();

    // Strip from the impl block itself (e.g., #[contract(expose = [...])])
    impl_block
        .attrs
        .retain(|attr| !attr.path().is_ident("contract"));

    // Strip from methods (e.g., #[contract(no_event)], #[contract(feeds = "...")])
    for item in &mut impl_block.items {
        if let ImplItem::Fn(method) = item {
            method
                .attrs
                .retain(|attr| !attr.path().is_ident("contract"));
        }
    }

    // For trait impls, remove empty-body methods so they use the default
    // implementation
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
    use crate::parse::{ParameterInfo, Receiver};
    use quote::ToTokens;

    fn normalize_tokens(tokens: &TokenStream2) -> String {
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
            returns_ref: false,
            receiver: Receiver::Ref,
            trait_name: None,
            feed_type: None,
        }];

        let output = normalize_tokens(&extern_wrappers(&functions, &contract_ident));

        let expected = normalize_tokens(&quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[unsafe(no_mangle)]
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
            returns_ref: false,
            receiver: Receiver::RefMut,
            trait_name: None,
            feed_type: None,
        }];

        let output = normalize_tokens(&extern_wrappers(&functions, &contract_ident));

        let expected = normalize_tokens(&quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[unsafe(no_mangle)]
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
            returns_ref: false,
            receiver: Receiver::RefMut,
            trait_name: None,
            feed_type: None,
        }];

        let output = normalize_tokens(&extern_wrappers(&functions, &contract_ident));

        let expected = normalize_tokens(&quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[unsafe(no_mangle)]
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
                returns_ref: false,
                receiver: Receiver::RefMut,
                trait_name: None,
                feed_type: None,
            },
        ];

        let output = normalize_tokens(&extern_wrappers(&functions, &contract_ident));

        let expected = normalize_tokens(&quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[unsafe(no_mangle)]
                unsafe extern "C" fn pause(arg_len: u32) -> u32 {
                    dusk_core::abi::wrap_call(arg_len, |(): ()| STATE.pause())
                }

                #[unsafe(no_mangle)]
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
            returns_ref: true,
            receiver: Receiver::Ref,
            trait_name: None,
            feed_type: None,
        }];

        let output = normalize_tokens(&extern_wrappers(&functions, &contract_ident));

        let expected = normalize_tokens(&quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[unsafe(no_mangle)]
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
            returns_ref: false,
            receiver: Receiver::RefMut,
            trait_name: None,
            feed_type: None,
        }];

        let output = normalize_tokens(&extern_wrappers(&functions, &contract_ident));

        let expected = normalize_tokens(&quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[unsafe(no_mangle)]
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
            returns_ref: false,
            receiver: Receiver::RefMut,
            trait_name: None,
            feed_type: None,
        }];

        let output = normalize_tokens(&extern_wrappers(&functions, &contract_ident));

        let expected = normalize_tokens(&quote! {
            #[cfg(target_family = "wasm")]
            mod __contract_extern_wrappers {
                use super::*;

                #[unsafe(no_mangle)]
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
        let output = normalize_tokens(&state_static(&contract_ident));

        let expected = normalize_tokens(&quote! {
            /// Static contract state initialized via `new()`.
            #[cfg(target_family = "wasm")]
            static mut STATE: MyContract = MyContract::new();
        });

        assert_eq!(expected, output);
    }

    #[test]
    fn test_local_const_imports_brace_wraps_const() {
        let module: ItemMod = syn::parse_quote! {
            mod my_contract {
                const DEPTH: usize = 16;
                pub struct MyContract;
            }
        };
        let items = &module.content.as_ref().unwrap().1;
        let mod_name = format_ident!("my_contract");
        let imports = local_const_imports(items, &mod_name);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].name, "DEPTH");
        assert_eq!(imports[0].path, "{ super::my_contract::DEPTH }");
        assert_eq!(imports[1].name, "MyContract");
        assert_eq!(imports[1].path, "super::my_contract::MyContract");
    }

    #[test]
    fn test_data_visible_items_skips_impl_only_use() {
        let module: ItemMod = syn::parse_quote! {
            mod my_contract {
                mod only_impl {
                    pub fn helper() -> u32 {
                        1
                    }
                }
                use only_impl::helper;
                const DEPTH: usize = 8;
                pub struct MyContract;
                impl MyContract {
                    pub fn touch(&mut self) {
                        let _ = helper();
                    }
                }
            }
        };
        let items = &module.content.as_ref().unwrap().1;

        let visible = data_visible_items(items);
        let has_helper_use = visible.iter().any(|item| {
            matches!(item, Item::Use(u) if u.to_token_stream().to_string().contains("helper"))
        });
        assert!(!has_helper_use, "impl-only use must not appear in data-visible items");
        assert!(
            visible.iter().any(|i| matches!(i, Item::Const(_))),
            "const DEPTH should remain visible"
        );
    }
}
