// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Import parsing functionality for the contract macro.

use syn::{ItemUse, UseTree};

use crate::{is_relative_path_keyword, ImportExtraction, ImportInfo};

/// Extract imports from a `use` statement.
pub(crate) fn imports_from_use(item_use: &ItemUse) -> ImportExtraction {
    extract_imports_from_tree(&item_use.tree, "")
}

/// Recursively extract imports from a use tree.
fn extract_imports_from_tree(tree: &UseTree, prefix: &str) -> ImportExtraction {
    match tree {
        UseTree::Path(path) => {
            // Check if this is a relative path (self::, super::, crate::)
            let is_relative =
                prefix.is_empty() && is_relative_path_keyword(&path.ident.to_string());

            // Build the path prefix
            let new_prefix = if prefix.is_empty() {
                path.ident.to_string()
            } else {
                format!("{prefix}::{}", path.ident)
            };
            let mut extraction = extract_imports_from_tree(&path.tree, &new_prefix);
            extraction.has_relative = extraction.has_relative || is_relative;
            extraction
        }
        UseTree::Name(name) => {
            // Final name: use foo::bar::Baz;
            let full_path = if prefix.is_empty() {
                name.ident.to_string()
            } else {
                format!("{prefix}::{}", name.ident)
            };
            ImportExtraction {
                imports: vec![ImportInfo {
                    name: name.ident.to_string(),
                    path: full_path,
                }],
                has_glob: false,
                has_relative: false,
            }
        }
        UseTree::Rename(rename) => {
            // Renamed import: use foo::bar::Baz as Qux;
            let full_path = if prefix.is_empty() {
                rename.ident.to_string()
            } else {
                format!("{prefix}::{}", rename.ident)
            };
            ImportExtraction {
                imports: vec![ImportInfo {
                    name: rename.rename.to_string(),
                    path: full_path,
                }],
                has_glob: false,
                has_relative: false,
            }
        }
        UseTree::Glob(_) => {
            // Glob import: use foo::*; - we can't resolve these
            ImportExtraction {
                imports: vec![],
                has_glob: true,
                has_relative: false,
            }
        }
        UseTree::Group(group) => {
            // Group: use foo::{Bar, Baz};
            let mut imports = Vec::new();
            let mut has_glob = false;
            let mut has_relative = false;
            for item in &group.items {
                let extraction = extract_imports_from_tree(item, prefix);
                imports.extend(extraction.imports);
                has_glob = has_glob || extraction.has_glob;
                has_relative = has_relative || extraction.has_relative;
            }
            ImportExtraction {
                imports,
                has_glob,
                has_relative,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_imports_simple() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use evm_core::standard_bridge::SetU64;
        };
        let extraction = imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 1);
        assert_eq!(extraction.imports[0].name, "SetU64");
        assert_eq!(
            extraction.imports[0].path,
            "evm_core::standard_bridge::SetU64"
        );
        assert!(!extraction.has_glob);
        assert!(!extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_renamed() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use dusk_core::Address as DSAddress;
        };
        let extraction = imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 1);
        assert_eq!(extraction.imports[0].name, "DSAddress");
        assert_eq!(extraction.imports[0].path, "dusk_core::Address");
        assert!(!extraction.has_glob);
        assert!(!extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_group() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use evm_core::standard_bridge::{SetU64, Deposit, EVMAddress};
        };
        let extraction = imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 3);
        assert!(!extraction.has_glob);
        assert!(!extraction.has_relative);

        let names: Vec<_> = extraction.imports.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"SetU64"));
        assert!(names.contains(&"Deposit"));
        assert!(names.contains(&"EVMAddress"));

        let set_u64 = extraction
            .imports
            .iter()
            .find(|i| i.name == "SetU64")
            .unwrap();
        assert_eq!(set_u64.path, "evm_core::standard_bridge::SetU64");
    }

    #[test]
    fn test_extract_imports_glob() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use evm_core::standard_bridge::*;
        };
        let extraction = imports_from_use(&use_stmt);
        assert!(extraction.imports.is_empty());
        assert!(extraction.has_glob);
        assert!(!extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_group_with_glob() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use evm_core::standard_bridge::{SetU64, events::*};
        };
        let extraction = imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 1);
        assert_eq!(extraction.imports[0].name, "SetU64");
        assert!(extraction.has_glob);
        assert!(!extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_relative_self() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use self::types::MyType;
        };
        let extraction = imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 1);
        assert_eq!(extraction.imports[0].name, "MyType");
        assert_eq!(extraction.imports[0].path, "self::types::MyType");
        assert!(!extraction.has_glob);
        assert!(extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_relative_super() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use super::common::SharedType;
        };
        let extraction = imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 1);
        assert_eq!(extraction.imports[0].name, "SharedType");
        assert_eq!(extraction.imports[0].path, "super::common::SharedType");
        assert!(!extraction.has_glob);
        assert!(extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_relative_crate() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use crate::utils::Helper;
        };
        let extraction = imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 1);
        assert_eq!(extraction.imports[0].name, "Helper");
        assert_eq!(extraction.imports[0].path, "crate::utils::Helper");
        assert!(!extraction.has_glob);
        assert!(extraction.has_relative);
    }

    #[test]
    fn test_extract_imports_group_with_relative() {
        let use_stmt: ItemUse = syn::parse_quote! {
            use self::types::{TypeA, TypeB};
        };
        let extraction = imports_from_use(&use_stmt);
        assert_eq!(extraction.imports.len(), 2);
        assert!(!extraction.has_glob);
        assert!(extraction.has_relative);
    }
}
