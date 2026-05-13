// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Phase 1 of the contract-macro pipeline: tokens -> IR.
//!
//! Each submodule owns one IR-producing concern:
//!
//! - [`model`]        IR types ([`Analysis`], [`FunctionInfo`], [`EventInfo`],
//!   …)
//! - [`imports`]      use-tree -> [`ImportInfo`]
//! - [`module`]       walks the user `mod {}` body
//! - [`functions`]    impl block -> [`FunctionInfo`] + method-level
//!   [`EventInfo`]s
//! - [`events`]       `abi::emit()` / `abi::feed()` discovery -> [`EventInfo`]
//! - [`directives`]   `#[contract(...)]` directive parser
//!
//! [`analyze`] is the orchestrator: it runs every submodule and returns a
//! fully-extracted, deduplicated [`Analysis`] that the `generate` phase
//! consumes. `lib.rs` only needs to call this one function.

mod directives;
mod events;
mod functions;
mod imports;
mod model;
mod module;

use syn::{Item, ItemImpl, ItemMod};

pub(crate) use self::model::{
    Analysis, EventInfo, FunctionInfo, ImportInfo, ParameterInfo, Receiver, TraitImplInfo,
};
use crate::validate;

/// Run the full parse phase against a `#[contract]` module.
///
/// Walks the module body, extracts imports, functions, events and method-level
/// emits, validates contract-level invariants, and returns an [`Analysis`]
/// with events deduplicated by topic. Returns the first encountered error.
pub(crate) fn analyze<'a>(module: &'a ItemMod, items: &'a [Item]) -> Result<Analysis, syn::Error> {
    let imports = module::imports(items)?;
    let struct_ = module::contract_struct(module, items)?;
    let contract_name = struct_.ident.to_string();

    let impl_blocks = module::impl_blocks(items, &contract_name);
    if impl_blocks.is_empty() {
        return Err(syn::Error::new_spanned(
            struct_,
            format!("#[contract] module must contain an impl block for `{contract_name}`"),
        ));
    }

    for impl_block in &impl_blocks {
        validate::impl_block_methods(impl_block)?;
    }

    validate::new_constructor(&contract_name, &impl_blocks, struct_)?;
    validate::init_method(&contract_name, &impl_blocks)?;

    let trait_impls = module::trait_impls(items, &contract_name)?;

    let mut functions = Vec::new();
    let mut events = Vec::new();

    for impl_block in &impl_blocks {
        let (block_functions, block_events) = extract_inherent_impl(impl_block)?;
        functions.extend(block_functions);
        events.extend(block_events);
    }

    for trait_impl in &trait_impls {
        let (trait_functions, trait_events) = extract_trait_impl(trait_impl)?;
        functions.extend(trait_functions);
        events.extend(trait_events);
    }

    let events = events::dedup_events_by_topic(events);

    Ok(Analysis {
        contract_ident: struct_.ident.clone(),
        contract_name,
        imports,
        functions,
        events,
    })
}

/// Extract functions and events from a single inherent impl block.
///
/// Merges the two sources of events: `abi::emit()` calls in method bodies
/// (discovered by `events::emit_calls`) and `#[contract(emits = [...])]`
/// attributes (already harvested by `functions::public_methods`). Body events
/// come first to match the order the schema was generated in pre-refactor.
fn extract_inherent_impl(
    impl_block: &ItemImpl,
) -> Result<(Vec<FunctionInfo>, Vec<EventInfo>), syn::Error> {
    let (block_functions, method_events) = functions::public_methods(impl_block)?;
    let mut block_events = events::emit_calls(impl_block);
    block_events.extend(method_events);
    Ok((block_functions, block_events))
}

/// Extract functions and events from a single trait impl block.
///
/// Mirrors [`extract_inherent_impl`] but routes through `trait_methods`,
/// which respects the `#[contract(expose = [...])]` filter.
fn extract_trait_impl(
    trait_impl: &TraitImplInfo,
) -> Result<(Vec<FunctionInfo>, Vec<EventInfo>), syn::Error> {
    let (trait_functions, method_events) = functions::trait_methods(trait_impl)?;
    let mut trait_events = events::emit_calls(trait_impl.impl_block);
    trait_events.extend(method_events);
    Ok((trait_functions, trait_events))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_no_impl_block() {
        let module: ItemMod = syn::parse_quote! {
            mod my_contract {
                pub struct MyContract {
                    value: u64,
                }
            }
        };
        let items = module.content.as_ref().unwrap().1.clone();

        let Err(err) = analyze(&module, &items) else {
            panic!("expected error for missing impl block");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("impl block"),
            "error should mention 'impl block': {msg}"
        );
        assert!(
            msg.contains("MyContract"),
            "error should mention contract name: {msg}"
        );
    }

    #[test]
    fn analyze_impl_for_different_type() {
        // Impl block exists but for the wrong type.
        let module: ItemMod = syn::parse_quote! {
            mod my_contract {
                pub struct MyContract {
                    value: u64,
                }
                struct Helper;
                impl Helper {
                    pub const fn new() -> Self { Self }
                }
            }
        };
        let items = module.content.as_ref().unwrap().1.clone();

        let Err(err) = analyze(&module, &items) else {
            panic!("expected error for impl on wrong type");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("impl block"),
            "error should mention 'impl block': {msg}"
        );
    }

    #[test]
    fn analyze_glob_import_rejected() {
        let module: ItemMod = syn::parse_quote! {
            mod my_contract {
                use some_crate::*;
                pub struct MyContract {
                    value: u64,
                }
                impl MyContract {
                    pub const fn new() -> Self { Self { value: 0 } }
                }
            }
        };
        let items = module.content.as_ref().unwrap().1.clone();

        let Err(err) = analyze(&module, &items) else {
            panic!("expected error for glob import");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("glob import"),
            "error should mention 'glob import': {msg}"
        );
    }

    #[test]
    fn analyze_relative_import_rejected() {
        let module: ItemMod = syn::parse_quote! {
            mod my_contract {
                use super::SomeType;
                pub struct MyContract {
                    value: u64,
                }
                impl MyContract {
                    pub const fn new() -> Self { Self { value: 0 } }
                }
            }
        };
        let items = module.content.as_ref().unwrap().1.clone();

        let Err(err) = analyze(&module, &items) else {
            panic!("expected error for relative import");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("relative import"),
            "error should mention 'relative import': {msg}"
        );
    }
}
