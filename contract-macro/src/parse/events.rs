// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Event handling: parsing the `#[contract(events = [...])]` module
//! attribute, validating `abi::emit()` call sites against the registered
//! set, and discovering `abi::feed()` call sites.

use std::collections::HashSet;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{
    Error as SynError, Expr, ExprCall, ExprPath, Ident, ImplItemFn, ItemImpl, Path, Token,
    bracketed, parse2,
};

use crate::parse::ImportInfo;
use crate::resolve;

/// Parse the `#[contract(events = [...])]` module attribute into the list of
/// registered event type paths.
///
/// An empty attribute (`#[contract]`) yields an empty list. Any other shape
/// must be `events = [path::A, path::B, ...]`; malformed input surfaces as a
/// span-anchored `syn::Error`.
pub(crate) fn module_events(attr: TokenStream2) -> Result<Vec<Path>, SynError> {
    if attr.is_empty() {
        return Ok(Vec::new());
    }
    let parsed: EventsAttr = parse2(attr)?;

    // The registered set is unique by construction; a repeated path would
    // double its schema entry and `decode_event` block.
    let mut seen = HashSet::new();
    for path in &parsed.paths {
        if !seen.insert(quote!(#path).to_string()) {
            return Err(SynError::new_spanned(
                path,
                format!(
                    "event type `{}` is registered more than once",
                    quote!(#path)
                ),
            ));
        }
    }

    Ok(parsed.paths)
}

/// The parsed `events = [...]` module attribute.
struct EventsAttr {
    paths: Vec<Path>,
}

impl Parse for EventsAttr {
    fn parse(input: ParseStream) -> Result<Self, SynError> {
        let keyword: Ident = input.parse()?;
        if keyword != "events" {
            return Err(SynError::new(
                keyword.span(),
                format!("unknown contract directive `{keyword}`; expected `events = [...]`"),
            ));
        }
        input
            .parse::<Token![=]>()
            .map_err(|_| SynError::new(keyword.span(), "expected `events = [Type, ...]`"))?;
        let content;
        bracketed!(content in input);
        let paths = Punctuated::<Path, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();
        Ok(Self { paths })
    }
}

/// Build the lookup set of registered event type paths, each resolved through
/// the module imports so emit-site types compare equal regardless of whether
/// they are written in qualified or imported short form.
pub(super) fn registered_keys(events: &[Path], imports: &[ImportInfo]) -> HashSet<String> {
    events
        .iter()
        .map(|p| resolve::resolve_type_str(&quote!(#p), imports))
        .collect()
}

/// Visitor collecting the data-argument expression of every `abi::emit()`
/// call so its type can be checked against the registered set.
struct EmitVisitor<'ast> {
    /// The second argument (event data) of each discovered emit call.
    data_exprs: Vec<&'ast Expr>,
}

impl<'ast> Visit<'ast> for EmitVisitor<'ast> {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Expr::Path(ExprPath { path, .. }) = &*node.func {
            let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();

            let is_emit = matches!(
                segments
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice(),
                ["abi", "emit"] | ["emit"]
            );

            if is_emit && node.args.len() >= 2 {
                self.data_exprs.push(&node.args[1]);
            }
        }

        syn::visit::visit_expr_call(self, node);
    }
}

/// Validate that every `abi::emit()` call inside an impl block emits a
/// registered event type.
///
/// The data argument's type path is extracted and compared against the set
/// declared in `#[contract(events = [...])]`. An emit of an unregistered type
/// produces a `compile_error!` anchored at the offending call. Topic
/// expressions are not checked here — decode-time scanning catches typos.
///
/// Only emits whose data argument is a concrete, nameable type path are
/// checked. A field access (`self.evt`), a function result (`build_event()`),
/// or a local binding (`let e = ..; emit(t, e)`) cannot be resolved to a type
/// syntactically, so they are skipped — an accepted false negative, mirroring
/// the way helpers outside the contract module are never walked. The
/// registered list stays the source of truth for the schema.
///
/// The emit-site type is resolved through `imports` before comparison, so it
/// matches the registered set whether written qualified or in imported short
/// form. Only impl blocks within the contract module are walked.
pub(super) fn validate_emitted_types(
    impl_block: &ItemImpl,
    registered: &HashSet<String>,
    imports: &[ImportInfo],
) -> Result<(), SynError> {
    let mut visitor = EmitVisitor {
        data_exprs: Vec::new(),
    };
    visitor.visit_item_impl(impl_block);

    for data_expr in visitor.data_exprs {
        let ty = type_from_expr(data_expr);
        if !is_checkable_type(&ty) {
            continue;
        }
        if !registered.contains(&resolve::resolve_type_str(&ty, imports)) {
            return Err(SynError::new_spanned(
                data_expr,
                format!(
                    "event type `{ty}` is emitted but not registered; \
                     add it to the `#[contract(events = [...])]` list"
                ),
            ));
        }
    }

    Ok(())
}

/// Decide whether an extracted emit-data type is a concrete type path worth
/// checking against the registered set.
///
/// Type names are conventionally upper-camel-case, so a path whose final
/// segment starts lowercase (a local binding, a free-function call, or a
/// primitive) is treated as unresolvable and skipped, as is the `()` fallback
/// `type_from_expr` returns for shapes it can't read (field access, etc.).
fn is_checkable_type(ty: &TokenStream2) -> bool {
    let Ok(path) = syn::parse2::<Path>(ty.clone()) else {
        return false;
    };
    path.segments
        .last()
        .and_then(|seg| seg.ident.to_string().chars().next())
        .is_some_and(char::is_uppercase)
}

/// Visitor to detect `abi::feed()` calls within function bodies.
struct FeedVisitor {
    /// The expressions passed to `abi::feed()` calls, as strings.
    feed_exprs: Vec<String>,
}

impl FeedVisitor {
    /// Create a new visitor.
    fn new() -> Self {
        Self {
            feed_exprs: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for FeedVisitor {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        // Check if this is an abi::feed() call
        if let Expr::Path(ExprPath { path, .. }) = &*node.func {
            let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();

            // Match abi::feed or just feed
            let is_feed = matches!(
                segments
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice(),
                ["abi", "feed"] | ["feed"]
            );

            if is_feed && !node.args.is_empty() {
                // Capture the expression being fed
                let expr = &node.args[0];
                let expr_str = quote!(#expr).to_string();
                self.feed_exprs.push(expr_str);
            }
        }

        // Continue visiting nested expressions
        syn::visit::visit_expr_call(self, node);
    }
}

/// Check if a method body contains `abi::feed()` calls.
/// Returns the expressions being fed (empty if no feed calls).
pub(super) fn get_feed_exprs(method: &ImplItemFn) -> Vec<String> {
    let mut visitor = FeedVisitor::new();
    visitor.visit_block(&method.block);
    visitor.feed_exprs
}

/// Check if a type string looks like a tuple (starts with `(` and contains
/// `,`).
fn looks_like_tuple(s: &str) -> bool {
    let trimmed = s.trim();
    trimmed.starts_with('(') && trimmed.contains(',')
}

/// Validate that the `feeds` attribute type matches the fed expressions.
/// Returns an error message if there's a mismatch, None if OK.
pub(super) fn validate_feed_type_match(
    feed_type_str: &str,
    feed_exprs: &[String],
) -> Option<String> {
    if feed_exprs.is_empty() {
        return None;
    }

    let feeds_is_tuple = looks_like_tuple(feed_type_str);

    // Check the first fed expression (they should all be the same type in practice)
    let expr = &feed_exprs[0];
    let expr_is_tuple = looks_like_tuple(expr);

    if feeds_is_tuple && !expr_is_tuple {
        Some(format!(
            "feeds attribute specifies tuple type `{feed_type_str}` but expression `{expr}` doesn't look like a tuple"
        ))
    } else if !feeds_is_tuple && expr_is_tuple {
        Some(format!(
            "feeds attribute specifies non-tuple type `{feed_type_str}` but expression `{expr}` looks like a tuple"
        ))
    } else {
        None
    }
}

/// Attempt to extract a type from an expression.
///
/// Handles the inline-construction shapes `Type { .. }`, `Type()`, and the
/// bare path `Type`. Constructor calls like `Type::new()` yield the path
/// `Type::new`, whose lowercase last segment makes [`is_checkable_type`] skip
/// it — an accepted false negative, since the type can't be read off the call.
pub(super) fn type_from_expr(expr: &Expr) -> TokenStream2 {
    match expr {
        // Handle struct instantiation: events::PauseToggled { ... } or PauseToggled { ... }
        Expr::Struct(s) => {
            let path = &s.path;
            quote! { #path }
        }
        // Handle unit struct or tuple struct: events::PauseToggled() or PauseToggled()
        Expr::Call(call) => {
            if let Expr::Path(path) = &*call.func {
                let p = &path.path;
                quote! { #p }
            } else {
                quote! { () }
            }
        }
        // Handle path expressions: events::PauseToggled
        Expr::Path(path) => {
            let p = &path.path;
            quote! { #p }
        }
        // Fallback - unknown type
        _ => quote! { () },
    }
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

    fn normalize_tokens(tokens: &TokenStream2) -> String {
        tokens
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    // =========================================================================
    // module_events parser
    // =========================================================================

    #[test]
    fn test_module_events_empty_attr() {
        let paths = module_events(TokenStream2::new()).unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn test_module_events_single() {
        let attr = quote! { events = [events::CounterReset] };
        let paths = module_events(attr).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(
            normalize_tokens(&quote!(#(#paths)*)),
            "events :: CounterReset"
        );
    }

    #[test]
    fn test_module_events_multiple_with_trailing_comma() {
        let attr = quote! { events = [A, b::C, d::e::F,] };
        let paths = module_events(attr).unwrap();
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn test_module_events_unknown_keyword() {
        let attr = quote! { emits = [A] };
        let Err(err) = module_events(attr) else {
            panic!("expected error for unknown keyword");
        };
        assert!(err.to_string().contains("unknown contract directive"));
    }

    #[test]
    fn test_module_events_missing_brackets() {
        let attr = quote! { events = A };
        assert!(module_events(attr).is_err());
    }

    #[test]
    fn test_module_events_duplicate_path_rejected() {
        let attr = quote! { events = [events::Foo, events::Foo] };
        let Err(err) = module_events(attr) else {
            panic!("expected error for duplicate registered event");
        };
        assert!(err.to_string().contains("registered more than once"));
    }

    // =========================================================================
    // validate_emitted_types
    // =========================================================================

    #[test]
    fn test_validate_emitted_types_registered_passes() {
        let impl_block: ItemImpl = parse_quote! {
            impl MyContract {
                pub fn pause(&mut self) {
                    abi::emit(events::Paused::TOPIC, events::Paused {});
                }
            }
        };
        let registered = registered_keys(&[parse_quote!(events::Paused)], &[]);
        assert!(validate_emitted_types(&impl_block, &registered, &[]).is_ok());
    }

    #[test]
    fn test_validate_emitted_types_unregistered_fails_with_span() {
        let impl_block: ItemImpl = parse_quote! {
            impl MyContract {
                pub fn pause(&mut self) {
                    abi::emit("paused", Unregistered {});
                }
            }
        };
        let registered = registered_keys(&[parse_quote!(events::Paused)], &[]);
        let err = validate_emitted_types(&impl_block, &registered, &[]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Unregistered"), "got: {msg}");
        assert!(msg.contains("not registered"), "got: {msg}");
    }

    #[test]
    fn test_validate_emitted_types_unit_struct_call() {
        let impl_block: ItemImpl = parse_quote! {
            impl MyContract {
                pub fn reset(&mut self) {
                    abi::emit(events::Reset::TOPIC, events::Reset());
                }
            }
        };
        let registered = registered_keys(&[parse_quote!(events::Reset)], &[]);
        assert!(validate_emitted_types(&impl_block, &registered, &[]).is_ok());
    }

    #[test]
    fn test_validate_emitted_types_nested_in_branch() {
        let impl_block: ItemImpl = parse_quote! {
            impl MyContract {
                pub fn maybe(&mut self, cond: bool) {
                    if cond {
                        abi::emit("t", Bad {});
                    }
                }
            }
        };
        let registered = registered_keys(&[parse_quote!(Good)], &[]);
        assert!(validate_emitted_types(&impl_block, &registered, &[]).is_err());
    }

    #[test]
    fn test_validate_emitted_types_skips_local_binding() {
        // Constructing the event in a local before emitting is a common
        // pattern; the data arg is then a bare lowercase ident the validator
        // can't resolve to a type, so it is skipped rather than rejected.
        let impl_block: ItemImpl = parse_quote! {
            impl MyContract {
                pub fn pause(&mut self) {
                    let event = events::Paused {};
                    abi::emit(events::Paused::TOPIC, event);
                }
            }
        };
        // Nothing registered — must still pass, since the local is skipped.
        let registered = registered_keys(&[], &[]);
        assert!(validate_emitted_types(&impl_block, &registered, &[]).is_ok());
    }

    #[test]
    fn test_validate_emitted_types_skips_field_access() {
        let impl_block: ItemImpl = parse_quote! {
            impl MyContract {
                pub fn pause(&mut self) {
                    abi::emit(events::Paused::TOPIC, self.pending_event);
                }
            }
        };
        let registered = registered_keys(&[], &[]);
        assert!(validate_emitted_types(&impl_block, &registered, &[]).is_ok());
    }

    #[test]
    fn test_validate_emitted_types_skips_function_result() {
        // A free-function call resolves to a lowercase callee path, not a
        // type, so it is skipped.
        let impl_block: ItemImpl = parse_quote! {
            impl MyContract {
                pub fn pause(&mut self) {
                    abi::emit(events::Paused::TOPIC, build_event());
                }
            }
        };
        let registered = registered_keys(&[], &[]);
        assert!(validate_emitted_types(&impl_block, &registered, &[]).is_ok());
    }

    #[test]
    fn test_validate_emitted_types_qualified_and_short_form_unify() {
        // Register the qualified path while emitting the imported short form.
        // Resolving both sides through the shared import makes them match, so
        // valid code is not falsely rejected.
        let imports = vec![ImportInfo {
            name: "events".to_string(),
            path: "crate::events".to_string(),
        }];
        let registered = registered_keys(&[parse_quote!(events::Paused)], &imports);

        let impl_block: ItemImpl = parse_quote! {
            impl MyContract {
                pub fn pause(&mut self) {
                    abi::emit("paused", events::Paused {});
                }
            }
        };
        assert!(validate_emitted_types(&impl_block, &registered, &imports).is_ok());
    }

    #[test]
    fn test_validate_emitted_types_same_type_multiple_topics() {
        // One struct emitted under two topics — still a single registered type.
        let impl_block: ItemImpl = parse_quote! {
            impl MyContract {
                pub fn add(&mut self, item: Item) {
                    abi::emit(Item::ADDED, Item { ..item });
                }
                pub fn remove(&mut self, item: Item) {
                    abi::emit(Item::REMOVED, Item { ..item });
                }
            }
        };
        let registered = registered_keys(&[parse_quote!(Item)], &[]);
        assert!(validate_emitted_types(&impl_block, &registered, &[]).is_ok());
    }

    // =========================================================================
    // type_from_expr
    // =========================================================================

    #[test]
    fn test_type_from_expr_struct() {
        let expr: Expr = parse_quote!(events::Paused { value: 1 });
        assert_eq!(normalize_tokens(&type_from_expr(&expr)), "events :: Paused");
    }

    #[test]
    fn test_type_from_expr_unit_call() {
        let expr: Expr = parse_quote!(Reset());
        assert_eq!(normalize_tokens(&type_from_expr(&expr)), "Reset");
    }

    #[test]
    fn test_type_from_expr_path() {
        let expr: Expr = parse_quote!(events::Singleton);
        assert_eq!(
            normalize_tokens(&type_from_expr(&expr)),
            "events :: Singleton"
        );
    }
}
