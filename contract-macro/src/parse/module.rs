// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Module-shape parsing: walking the user `mod { ... }` body to extract
//! imports, the contract struct, inherent impl blocks, and trait impl blocks
//! that carry a `#[contract(expose = [...])]` attribute.

use syn::{Item, ItemImpl, ItemMod, Type, Visibility};

use crate::parse::{directives, imports as imports_parse};
use crate::{ImportInfo, TraitImplInfo};

/// Extract and validate imports from the module items.
///
/// Returns an error if glob or relative imports are found.
pub(super) fn imports(items: &[Item]) -> Result<Vec<ImportInfo>, syn::Error> {
    let mut result = Vec::new();
    let mut glob_import = None;
    let mut relative_import = None;

    for item in items {
        if let Item::Use(item_use) = item {
            let extraction = imports_parse::imports_from_use(item_use);
            result.extend(extraction.imports);
            if extraction.has_glob && glob_import.is_none() {
                glob_import = Some(item_use);
            }
            if extraction.has_relative && relative_import.is_none() {
                relative_import = Some(item_use);
            }
        }
    }

    if let Some(item_use) = glob_import {
        return Err(syn::Error::new_spanned(
            item_use,
            "#[contract] does not support glob imports (`use foo::*`); \
             import types explicitly so their paths can be tracked",
        ));
    }

    if let Some(item_use) = relative_import {
        return Err(syn::Error::new_spanned(
            item_use,
            "#[contract] does not support relative imports (`use self::`, `use super::`, `use crate::`); \
             use absolute paths so they can be resolved for code generation",
        ));
    }

    Ok(result)
}

/// Find the contract struct in the module.
///
/// The module must contain exactly one `pub struct` which serves as the
/// contract state. Returns an error if there are zero or multiple public
/// structs.
pub(super) fn contract_struct<'a>(
    module: &'a ItemMod,
    items: &'a [Item],
) -> Result<&'a syn::ItemStruct, syn::Error> {
    let pub_structs: Vec<_> = items
        .iter()
        .filter_map(|item| {
            if let Item::Struct(s) = item
                && matches!(s.vis, Visibility::Public(_))
            {
                Some(s)
            } else {
                None
            }
        })
        .collect();

    if pub_structs.is_empty() {
        return Err(syn::Error::new_spanned(
            module,
            "#[contract] module must contain a pub struct for the contract state",
        ));
    }

    if pub_structs.len() > 1 {
        return Err(syn::Error::new_spanned(
            pub_structs[1],
            "#[contract] module must contain exactly one pub struct; \
             found multiple public structs",
        ));
    }

    Ok(pub_structs[0])
}

/// Find inherent impl blocks for the contract struct.
///
/// Returns all `impl ContractName { ... }` blocks (without a trait).
pub(super) fn impl_blocks<'a>(items: &'a [Item], contract_name: &str) -> Vec<&'a ItemImpl> {
    items
        .iter()
        .filter_map(|item| {
            if let Item::Impl(impl_block) = item
                && impl_block.trait_.is_none()
                && let Type::Path(type_path) = &*impl_block.self_ty
                && type_path.path.is_ident(contract_name)
            {
                Some(impl_block)
            } else {
                None
            }
        })
        .collect()
}

/// Find trait impl blocks with `#[contract(expose = [...])]` attributes.
///
/// Only trait implementations that have an explicit expose list are returned.
/// The expose list specifies which trait methods should have extern wrappers
/// generated.
pub(super) fn trait_impls<'a>(items: &'a [Item], contract_name: &str) -> Vec<TraitImplInfo<'a>> {
    items
        .iter()
        .filter_map(|item| {
            if let Item::Impl(impl_block) = item
                && let Some((_, trait_path, _)) = &impl_block.trait_
                && let Type::Path(type_path) = &*impl_block.self_ty
                && type_path.path.is_ident(contract_name)
                && let Some(list) = directives::expose_list(&impl_block.attrs)
            {
                let trait_name = trait_path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                Some(TraitImplInfo {
                    trait_name,
                    impl_block,
                    expose_list: list,
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_struct_no_public_struct() {
        let module: ItemMod = syn::parse_quote! {
            mod my_contract {
                struct PrivateState {
                    value: u64,
                }
            }
        };
        let items = module.content.as_ref().unwrap().1.clone();

        let result = contract_struct(&module, &items);
        let Err(err) = result else {
            panic!("expected error for no public struct");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("pub struct"),
            "error should mention 'pub struct': {msg}"
        );
    }

    #[test]
    fn test_contract_struct_only_private_structs() {
        let module: ItemMod = syn::parse_quote! {
            mod my_contract {
                struct PrivateOne {
                    a: u64,
                }
                struct PrivateTwo {
                    b: u64,
                }
            }
        };
        let items = module.content.as_ref().unwrap().1.clone();

        let result = contract_struct(&module, &items);
        let Err(err) = result else {
            panic!("expected error for only private structs");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("pub struct"),
            "error should mention 'pub struct': {msg}"
        );
    }

    #[test]
    fn test_contract_struct_multiple_public_structs() {
        let module: ItemMod = syn::parse_quote! {
            mod my_contract {
                pub struct ContractOne {
                    a: u64,
                }
                pub struct ContractTwo {
                    b: u64,
                }
            }
        };
        let items = module.content.as_ref().unwrap().1.clone();

        let result = contract_struct(&module, &items);
        let Err(err) = result else {
            panic!("expected error for multiple public structs");
        };
        let msg = err.to_string();
        assert!(
            msg.contains("exactly one pub struct"),
            "error should mention 'exactly one pub struct': {msg}"
        );
        assert!(
            msg.contains("multiple"),
            "error should mention 'multiple': {msg}"
        );
    }

    #[test]
    fn test_impl_blocks_finds_multiple() {
        let items: Vec<Item> = vec![
            syn::parse_quote! {
                impl MyContract {
                    pub fn method_a(&self) -> u64 { 0 }
                }
            },
            syn::parse_quote! {
                impl MyContract {
                    pub fn method_b(&self) -> u64 { 1 }
                }
            },
        ];

        let blocks = impl_blocks(&items, "MyContract");
        assert_eq!(blocks.len(), 2, "should find both impl blocks");
    }

    #[test]
    fn test_impl_blocks_ignores_trait_impls() {
        let items: Vec<Item> = vec![
            syn::parse_quote! {
                impl MyContract {
                    pub fn method_a(&self) -> u64 { 0 }
                }
            },
            syn::parse_quote! {
                impl SomeTrait for MyContract {
                    fn trait_method(&self) {}
                }
            },
        ];

        let blocks = impl_blocks(&items, "MyContract");
        assert_eq!(blocks.len(), 1, "should only find inherent impl block");
    }

    #[test]
    fn test_trait_impls_finds_with_expose() {
        let items: Vec<Item> = vec![syn::parse_quote! {
            #[contract(expose = [owner])]
            impl OwnableTrait for MyContract {
                fn owner(&self) -> Address { self.owner }
            }
        }];

        let trait_impls = trait_impls(&items, "MyContract");
        assert_eq!(trait_impls.len(), 1);
        assert_eq!(trait_impls[0].trait_name, "OwnableTrait");
        assert_eq!(trait_impls[0].expose_list, vec!["owner"]);
    }

    #[test]
    fn test_trait_impls_ignores_without_expose() {
        let items: Vec<Item> = vec![syn::parse_quote! {
            impl OwnableTrait for MyContract {
                fn owner(&self) -> Address { self.owner }
            }
        }];

        let trait_impls = trait_impls(&items, "MyContract");
        assert_eq!(
            trait_impls.len(),
            0,
            "should not find trait impl without expose attribute"
        );
    }

    #[test]
    fn test_trait_impls_multiple_traits() {
        let items: Vec<Item> = vec![
            syn::parse_quote! {
                #[contract(expose = [owner])]
                impl OwnableTrait for MyContract {
                    fn owner(&self) -> Address { self.owner }
                }
            },
            syn::parse_quote! {
                #[contract(expose = [version])]
                impl ISemver for MyContract {
                    fn version(&self) -> String { "1.0".to_string() }
                }
            },
        ];

        let trait_impls = trait_impls(&items, "MyContract");
        assert_eq!(trait_impls.len(), 2);
    }
}
