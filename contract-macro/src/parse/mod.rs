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
//! - [`functions`]    impl block -> [`FunctionInfo`]
//! - [`events`]       `events = [...]` attribute parsing, `abi::emit()`
//!   validation, `abi::feed()` discovery
//! - [`directives`]   `#[contract(...)]` directive parser
//!
//! [`analyze`] is the orchestrator: it runs every submodule and returns a
//! fully-extracted [`Analysis`] that the `generate` phase consumes. `lib.rs`
//! only needs to call this one function.

mod directives;
pub(crate) use directives::parse_contract_directives;
pub(crate) mod events;
mod functions;
mod imports;
mod model;
mod module;

use quote::quote;
use syn::{Item, ItemMod, Path};

pub(crate) use self::model::{
    Analysis, EventInfo, FunctionInfo, ImportInfo, ParameterInfo, Receiver,
};
use crate::validate;

/// Run the full parse phase against a `#[contract]` module.
///
/// Walks the module body, extracts imports and functions, validates
/// contract-level invariants, and checks every in-module `abi::emit()` call
/// against the events declared in the module attribute. Returns an
/// [`Analysis`] whose events are the registered list verbatim. Returns the
/// first encountered error.
pub(crate) fn analyze<'a>(
    module: &'a ItemMod,
    items: &'a [Item],
    registered_events: &[Path],
) -> Result<Analysis, syn::Error> {
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

    // Validate that every in-module emit references a registered event type.
    // Both sides are resolved through the module imports before comparison.
    let registered_keys = events::registered_keys(registered_events, &imports);
    for impl_block in &impl_blocks {
        events::validate_emitted_types(impl_block, &registered_keys, &imports)?;
    }
    for trait_impl in &trait_impls {
        events::validate_emitted_types(trait_impl.impl_block, &registered_keys, &imports)?;
    }

    let mut functions = Vec::new();
    for impl_block in &impl_blocks {
        functions.extend(functions::public_methods(impl_block)?);
    }
    for trait_impl in &trait_impls {
        functions.extend(functions::trait_methods(trait_impl)?);
    }

    let events = registered_events
        .iter()
        .map(|path| EventInfo {
            data_type: quote! { #path },
        })
        .collect();

    Ok(Analysis {
        contract_ident: struct_.ident.clone(),
        contract_name,
        imports,
        functions,
        events,
    })
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

        let Err(err) = analyze(&module, &items, &[]) else {
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

        let Err(err) = analyze(&module, &items, &[]) else {
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

        let Err(err) = analyze(&module, &items, &[]) else {
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

        let Err(err) = analyze(&module, &items, &[]) else {
            panic!("expected error for relative import");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("relative import"),
            "error should mention 'relative import': {msg}"
        );
    }
}
