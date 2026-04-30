// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Phase 1 of the contract-macro pipeline: tokens -> IR.
//!
//! Each submodule owns one IR-producing concern:
//!
//! - [`imports`]      use-tree -> [`crate::ImportInfo`]
//! - [`module`]       walks the user `mod {}` body
//! - [`functions`]    impl block -> [`crate::FunctionInfo`] /
//!   [`crate::ParameterInfo`]
//! - [`events`]       `abi::emit()` / `abi::feed()` discovery ->
//!   [`crate::EventInfo`]
//! - [`directives`]   `#[contract(...)]` directive parsers
//!
//! The [`contract_data`] orchestrator below is the entry point used by
//! `lib.rs`.

mod directives;
mod events;
mod functions;
mod imports;
mod module;

pub(crate) use events::{
    dedup_events_by_topic, emit_calls, inherent_method_emits, trait_method_emits,
};
pub(crate) use functions::{public_methods, trait_methods};
use syn::{Item, ItemMod};

use crate::{ContractData, validate};

/// Extract contract data from the module, validating constraints.
///
/// Returns an error if validation fails.
pub(crate) fn contract_data<'a>(
    module: &'a ItemMod,
    items: &'a [Item],
) -> Result<ContractData<'a>, syn::Error> {
    let imports = module::imports(items)?;
    let struct_ = module::contract_struct(module, items)?;
    let name = struct_.ident.to_string();

    let impl_blocks = module::impl_blocks(items, &name);
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

    let trait_impls = module::trait_impls(items, &name);

    Ok(ContractData {
        imports,
        contract_name: name,
        contract_ident: struct_.ident.clone(),
        impl_blocks,
        trait_impls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_data_no_impl_block() {
        let module: ItemMod = syn::parse_quote! {
            mod my_contract {
                pub struct MyContract {
                    value: u64,
                }
            }
        };
        let items = module.content.as_ref().unwrap().1.clone();

        let result = contract_data(&module, &items);
        let Err(err) = result else {
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
    fn test_contract_data_impl_for_different_type() {
        // Impl block exists but for wrong type
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

        let result = contract_data(&module, &items);
        let Err(err) = result else {
            panic!("expected error for impl on wrong type");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("impl block"),
            "error should mention 'impl block': {msg}"
        );
    }

    #[test]
    fn test_contract_data_glob_import_rejected() {
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

        let result = contract_data(&module, &items);
        let Err(err) = result else {
            panic!("expected error for glob import");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("glob import"),
            "error should mention 'glob import': {msg}"
        );
    }

    #[test]
    fn test_contract_data_relative_import_rejected() {
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

        let result = contract_data(&module, &items);
        let Err(err) = result else {
            panic!("expected error for relative import");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("relative import"),
            "error should mention 'relative import': {msg}"
        );
    }
}
