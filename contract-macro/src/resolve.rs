// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Type path resolution using import information.
//!
//! This module resolves short type names (as used in code) to their fully
//! qualified paths using the collected import information.

use std::collections::HashMap;

use proc_macro2::TokenStream as TokenStream2;

use crate::parse::ImportInfo;

/// A map from type names (as used in code) to their fully qualified paths.
pub(crate) type TypeMap = HashMap<String, String>;

/// Build an import lookup map from short name/alias to full path.
fn build_import_map(imports: &[ImportInfo]) -> HashMap<String, String> {
    imports
        .iter()
        .map(|i| (i.name.clone(), i.path.clone()))
        .collect()
}

/// Resolve a type path to its fully qualified form.
///
/// Given a type like `Deposit` or `events::PauseToggled` and an import map,
/// returns the fully qualified path like `my_crate::Deposit`
/// or `my_crate::events::PauseToggled`.
///
/// Handles:
/// - Simple types: `Deposit` -> `my_crate::Deposit`
/// - Aliased types: `DSAddress` -> `my_crate::Address`
/// - Multi-segment paths: `events::PauseToggled` ->
///   `my_crate::events::PauseToggled`
/// - Generic types: `Option<Deposit>` -> `Option<my_crate::Deposit>`
fn resolve_type(ty: &TokenStream2, import_map: &HashMap<String, String>) -> String {
    let ty_str = ty.to_string();

    // Handle unit type
    if ty_str == "()" {
        return "()".to_string();
    }

    // Try to parse as a syn::Type for proper handling
    if let Ok(parsed) = syn::parse2::<syn::Type>(ty.clone()) {
        return resolve_syn_type(&parsed, import_map);
    }

    // Fallback: return as-is
    ty_str
}

/// Resolve a `syn::Type` to its fully qualified string form.
fn resolve_syn_type(ty: &syn::Type, import_map: &HashMap<String, String>) -> String {
    match ty {
        syn::Type::Path(type_path) => resolve_type_path(type_path, import_map),
        syn::Type::Tuple(tuple) => {
            let resolved: Vec<_> = tuple
                .elems
                .iter()
                .map(|elem| resolve_syn_type(elem, import_map))
                .collect();
            format!("({})", resolved.join(", "))
        }
        syn::Type::Reference(reference) => {
            let resolved = resolve_syn_type(&reference.elem, import_map);
            if reference.mutability.is_some() {
                format!("&mut {resolved}")
            } else {
                format!("&{resolved}")
            }
        }
        _ => quote::quote!(#ty).to_string(),
    }
}

/// Resolve a `TypePath` to its fully qualified string form.
fn resolve_type_path(type_path: &syn::TypePath, import_map: &HashMap<String, String>) -> String {
    let path = &type_path.path;
    let segments: Vec<_> = path.segments.iter().collect();

    if segments.is_empty() {
        return quote::quote!(#path).to_string();
    }

    let first_seg = &segments[0];
    let first_name = first_seg.ident.to_string();

    // Check if the first segment can be resolved via imports
    if let Some(resolved_base) = import_map.get(&first_name) {
        // Build the rest of the path
        let rest: Vec<String> = segments[1..]
            .iter()
            .map(|seg| format_segment(seg, import_map))
            .collect();

        if rest.is_empty() {
            // Single segment type, may have generics
            let generics = format_generic_args(&first_seg.arguments, import_map);
            format!("{resolved_base}{generics}")
        } else {
            // Multi-segment path: resolved_base::rest[0]::rest[1]::...
            format!("{}::{}", resolved_base, rest.join("::"))
        }
    } else {
        // Not in import map - format as-is but still resolve generics
        let formatted: Vec<String> = segments
            .iter()
            .map(|seg| format_segment(seg, import_map))
            .collect();
        formatted.join("::")
    }
}

/// Format a path segment, resolving any generic arguments.
fn format_segment(seg: &syn::PathSegment, import_map: &HashMap<String, String>) -> String {
    let name = seg.ident.to_string();
    let generics = format_generic_args(&seg.arguments, import_map);
    format!("{name}{generics}")
}

/// Format generic arguments, resolving inner types.
fn format_generic_args(args: &syn::PathArguments, import_map: &HashMap<String, String>) -> String {
    match args {
        syn::PathArguments::None => String::new(),
        syn::PathArguments::AngleBracketed(angle) => {
            let resolved: Vec<String> = angle
                .args
                .iter()
                .map(|arg| match arg {
                    syn::GenericArgument::Type(ty) => resolve_syn_type(ty, import_map),
                    other => quote::quote!(#other).to_string(),
                })
                .collect();
            format!("<{}>", resolved.join(", "))
        }
        syn::PathArguments::Parenthesized(paren) => {
            let inputs: Vec<String> = paren
                .inputs
                .iter()
                .map(|ty| resolve_syn_type(ty, import_map))
                .collect();
            let output = match &paren.output {
                syn::ReturnType::Default => String::new(),
                syn::ReturnType::Type(_, ty) => format!(" -> {}", resolve_syn_type(ty, import_map)),
            };
            format!("({}){}", inputs.join(", "), output)
        }
    }
}

/// Build a type map containing all types used in functions and events,
/// resolved to their fully qualified paths.
pub(crate) fn build_type_map(
    imports: &[ImportInfo],
    functions: &[crate::parse::FunctionInfo],
    events: &[crate::parse::EventInfo],
) -> TypeMap {
    let import_map = build_import_map(imports);
    let mut type_map = TypeMap::new();

    // Resolve function input, output, and feed types
    for func in functions {
        let input_key = func.input_type.to_string();
        let input_resolved = resolve_type(&func.input_type, &import_map);
        type_map.insert(input_key, input_resolved);

        let output_key = func.output_type.to_string();
        let output_resolved = resolve_type(&func.output_type, &import_map);
        type_map.insert(output_key, output_resolved);

        // Resolve feed_type if present (from #[contract(feeds = "Type")])
        if let Some(feed_type) = &func.feed_type {
            let feed_key = feed_type.to_string();
            let feed_resolved = resolve_type(feed_type, &import_map);
            type_map.insert(feed_key, feed_resolved);
        }
    }

    // Resolve event data types
    for event in events {
        let data_key = event.data_type.to_string();
        let data_resolved = resolve_type(&event.data_type, &import_map);
        type_map.insert(data_key, data_resolved);
    }

    type_map
}

/// Resolve a type's tokens to their fully-qualified string form using the
/// module's imports.
///
/// Used by the emit validator so that the registered list and the emit site
/// are compared after the same import rewriting — `events::Foo` and the
/// imported short form `Foo` normalize to the same string when a shared
/// `use` covers them.
pub(crate) fn resolve_type_str(ty: &TokenStream2, imports: &[ImportInfo]) -> String {
    let import_map = build_import_map(imports);
    resolve_type(ty, &import_map)
}

/// Get the resolved type tokens for `ty` from the `type_map`, falling back to
/// `ty` unchanged when no resolution is recorded.
///
/// The resolved string is re-parsed as a [`syn::Type`] (not a `Path`, since
/// tuples are not paths) so the caller receives usable tokens.
pub(crate) fn resolved_tokens(ty: &TokenStream2, type_map: &TypeMap) -> TokenStream2 {
    let key = ty.to_string();
    if let Some(resolved) = type_map.get(&key)
        && let Ok(resolved_type) = syn::parse_str::<syn::Type>(resolved)
    {
        return quote::quote! { #resolved_type };
    }
    ty.clone()
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::*;

    fn make_import(name: &str, path: &str) -> ImportInfo {
        ImportInfo {
            name: name.to_string(),
            path: path.to_string(),
        }
    }

    #[test]
    fn test_resolve_simple_type() {
        let imports = vec![make_import("Deposit", "my_crate::Deposit")];
        let import_map = build_import_map(&imports);

        let ty = quote! { Deposit };
        let resolved = resolve_type(&ty, &import_map);
        assert_eq!(resolved, "my_crate::Deposit");
    }

    #[test]
    fn test_resolve_aliased_type() {
        let imports = vec![make_import("DSAddress", "my_crate::Address")];
        let import_map = build_import_map(&imports);

        let ty = quote! { DSAddress };
        let resolved = resolve_type(&ty, &import_map);
        assert_eq!(resolved, "my_crate::Address");
    }

    #[test]
    fn test_resolve_multi_segment_path() {
        let imports = vec![make_import("events", "my_crate::events")];
        let import_map = build_import_map(&imports);

        let ty = quote! { events::PauseToggled };
        let resolved = resolve_type(&ty, &import_map);
        assert_eq!(resolved, "my_crate::events::PauseToggled");
    }

    #[test]
    fn test_resolve_generic_type() {
        let imports = vec![make_import("Deposit", "my_crate::Deposit")];
        let import_map = build_import_map(&imports);

        let ty = quote! { Option<Deposit> };
        let resolved = resolve_type(&ty, &import_map);
        assert_eq!(resolved, "Option<my_crate::Deposit>");
    }

    #[test]
    fn test_resolve_tuple_type() {
        let imports = vec![
            make_import("Deposit", "my_crate::Deposit"),
            make_import("DSAddress", "my_crate::Address"),
        ];
        let import_map = build_import_map(&imports);

        let ty = quote! { (Deposit, DSAddress) };
        let resolved = resolve_type(&ty, &import_map);
        assert_eq!(resolved, "(my_crate::Deposit, my_crate::Address)");
    }

    #[test]
    fn test_resolve_unit_type() {
        let imports = vec![];
        let import_map = build_import_map(&imports);

        let ty = quote! { () };
        let resolved = resolve_type(&ty, &import_map);
        assert_eq!(resolved, "()");
    }

    #[test]
    fn test_resolve_primitive_unchanged() {
        let imports = vec![];
        let import_map = build_import_map(&imports);

        let ty = quote! { u64 };
        let resolved = resolve_type(&ty, &import_map);
        assert_eq!(resolved, "u64");
    }

    // =========================================================================
    // resolved_tokens
    // =========================================================================

    fn normalize_tokens(tokens: &TokenStream2) -> String {
        tokens
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn test_resolved_tokens_found_in_map() {
        let mut type_map = TypeMap::new();
        type_map.insert("Address".to_string(), "my_crate::Address".to_string());

        let resolved = resolved_tokens(&quote! { Address }, &type_map);
        assert_eq!(normalize_tokens(&resolved), "my_crate :: Address");
    }

    #[test]
    fn test_resolved_tokens_not_in_map_returns_input() {
        let type_map = TypeMap::new();
        let resolved = resolved_tokens(&quote! { u64 }, &type_map);
        assert_eq!(normalize_tokens(&resolved), "u64");
    }

    #[test]
    fn test_resolved_tokens_tuple() {
        let mut type_map = TypeMap::new();
        type_map.insert(
            "(Address , u64)".to_string(),
            "(my_crate::Address, u64)".to_string(),
        );

        let resolved = resolved_tokens(&quote! { (Address, u64) }, &type_map);
        assert!(normalize_tokens(&resolved).contains("my_crate :: Address"));
    }

    // =========================================================================
    // resolve_type fallback
    //
    // When `syn::parse2::<syn::Type>` fails on the input tokens, `resolve_type`
    // returns `ty.to_string()` as a last resort. The fallback is safe today
    // but untested — a regression that flipped it (panic, empty string) would
    // not be caught until schema generation broke downstream.
    // =========================================================================

    #[test]
    fn test_resolve_type_malformed_input_returns_input_unchanged() {
        // A token stream that does not parse as a `syn::Type` — `let x = 5`
        // is a statement, not a type — must round-trip through `resolve_type`
        // unchanged. No panic, no `Err`, no empty string.
        let import_map = HashMap::new();
        let ty: TokenStream2 = quote! { let x = 5 };

        let resolved = resolve_type(&ty, &import_map);
        assert_eq!(
            resolved,
            ty.to_string(),
            "fallback returns the raw token-stream string when parsing as Type fails"
        );
    }

    #[test]
    fn test_build_type_map_preserves_unparseable_function_input_type() {
        // Companion: ensure the fallback survives the trip through
        // `build_type_map`. If a function's input type happens to be
        // unparseable, downstream consumers (schema, data-driver) must still
        // see the original token-string verbatim — the fallback string is
        // stored as the resolved value.
        let imports = vec![];
        let func = crate::parse::FunctionInfo {
            name: quote::format_ident!("malformed"),
            doc: None,
            params: vec![],
            input_type: quote! { let bad = 1 },
            output_type: quote! { () },
            returns_ref: false,
            receiver: crate::parse::Receiver::Ref,
            trait_name: None,
            feed_type: None,
            wasm_export_name: None,
        };

        let type_map = build_type_map(&imports, std::slice::from_ref(&func), &[]);

        let key = func.input_type.to_string();
        assert_eq!(
            type_map.get(&key).map(String::as_str),
            Some(key.as_str()),
            "build_type_map preserves the original token-string when resolve_type falls back"
        );
    }
}
