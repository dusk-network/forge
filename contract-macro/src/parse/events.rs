// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Event extraction from impl blocks: `abi::emit()` call-site discovery,
//! `abi::feed()` call-site discovery, and `#[contract(emits = [...])]`
//! attribute collection.

use std::collections::HashSet;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::visit::Visit;
use syn::{
    Attribute, Expr, ExprCall, ExprLit, ExprPath, ImplItem, ImplItemFn, ItemImpl, Lit, Visibility,
};

use crate::parse::directives;
use crate::{EventInfo, TraitImplInfo};

/// Visitor to find `abi::emit()` calls within function bodies.
struct EmitVisitor {
    /// Collected events.
    events: Vec<EventInfo>,
}

impl EmitVisitor {
    /// Create a new empty visitor.
    fn new() -> Self {
        Self { events: Vec::new() }
    }
}

impl<'ast> Visit<'ast> for EmitVisitor {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        // Check if this is an abi::emit() call
        if let Expr::Path(ExprPath { path, .. }) = &*node.func {
            let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();

            // Match abi::emit or just emit
            let is_emit = matches!(
                segments
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice(),
                ["abi", "emit"] | ["emit"]
            );

            if is_emit && node.args.len() >= 2 {
                // First arg is the topic - can be a string literal or a const path
                let topic = topic_from_expr(node.args.first().unwrap());

                if let Some(topic) = topic {
                    // Second arg is the event data - extract its type
                    let data_expr = &node.args[1];
                    let data_type = type_from_expr(data_expr);

                    self.events.push(EventInfo { topic, data_type });
                }
            }
        }

        // Continue visiting nested expressions
        syn::visit::visit_expr_call(self, node);
    }
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

/// Deduplicate a list of events by topic, keeping the first occurrence.
///
/// Two events sharing a topic but registering structurally different data
/// types collapse to the first-seen entry; the rest are dropped silently
/// (no diagnostic, no panic). Iteration order is preserved, so the result
/// is deterministic regardless of `HashSet`'s random seed.
pub(crate) fn dedup_events_by_topic(events: Vec<EventInfo>) -> Vec<EventInfo> {
    let mut seen = HashSet::new();
    events
        .into_iter()
        .filter(|e| seen.insert(e.topic.clone()))
        .collect()
}

/// Extract topic string from the first argument of `abi::emit()`.
///
/// Handles both string literals and const path expressions.
/// Detects when a lowercase single-segment path (likely a variable) is used as
/// a topic, since the macro can only capture the variable name, not its value.
pub(super) fn topic_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        // String literal: "topic_name"
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => Some(s.value()),
        // Path expression: Type::TOPIC or module::Type::TOPIC or variable
        Expr::Path(path) => {
            let segments: Vec<_> = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();

            // Single lowercase identifier is likely a variable, not a const.
            // e.g., `let topic = "foo"; abi::emit(topic, data);` — we can only
            // capture "topic" as the schema topic, not its runtime value.
            if segments.len() == 1 {
                let first_char = segments[0].chars().next();
                if first_char.is_some_and(char::is_lowercase) {
                    emit_variable_topic_warning(&segments[0]);
                }
            }

            Some(segments.join("::"))
        }
        _ => None,
    }
}

/// Emit a warning when a variable is used as an event topic.
///
/// Currently a no-op: `proc_macro::Diagnostic` requires nightly
/// (`proc_macro_diagnostic`). The detection logic in `topic_from_expr`
/// still identifies variable topics and unit tests verify the behaviour;
/// the warning can be enabled once the feature stabilises.
fn emit_variable_topic_warning(_name: &str) {}

/// Attempt to extract a type from an expression.
/// This handles common patterns like `Type { .. }`, `Type()`, `Type::new()`.
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

/// Extract all `abi::emit()` calls from an impl block.
///
/// Events are deduplicated by topic, keeping only the first occurrence.
pub(crate) fn emit_calls(impl_block: &ItemImpl) -> Vec<EventInfo> {
    let mut visitor = EmitVisitor::new();
    visitor.visit_item_impl(impl_block);

    dedup_events_by_topic(visitor.events)
}

/// Check if a method body contains any `abi::emit()` call.
pub(super) fn method_has_emit_call(method: &ImplItemFn) -> bool {
    let mut visitor = EmitVisitor::new();
    visitor.visit_block(&method.block);
    !visitor.events.is_empty()
}

/// Extract events from a method's `#[contract(emits = [...])]` attribute.
///
/// Returns the events registered on this specific method, or an empty vec if
/// none.
pub(super) fn method_emits(attrs: &[Attribute]) -> Vec<EventInfo> {
    directives::emits_list(attrs)
        .map(|events| {
            events
                .into_iter()
                .map(|(topic, data_type)| EventInfo { topic, data_type })
                .collect()
        })
        .unwrap_or_default()
}

/// Collect events from method-level `#[contract(emits = [...])]` attributes
/// on the methods of an impl block, restricted to those matching `include`.
fn impl_method_emits<F>(impl_block: &ItemImpl, mut include: F) -> Vec<EventInfo>
where
    F: FnMut(&ImplItemFn) -> bool,
{
    let mut events = Vec::new();
    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item
            && include(method)
        {
            events.extend(method_emits(&method.attrs));
        }
    }
    events
}

/// Extract events from method-level `#[contract(emits = [...])]` attributes in
/// a trait impl.
///
/// Only methods in the `expose_list` are checked for emits attributes.
pub(crate) fn trait_method_emits(trait_impl: &TraitImplInfo) -> Vec<EventInfo> {
    impl_method_emits(trait_impl.impl_block, |method| {
        trait_impl
            .expose_list
            .contains(&method.sig.ident.to_string())
    })
}

/// Extract events from method-level `#[contract(emits = [...])]` attributes in
/// an inherent impl block.
///
/// Only public methods (excluding `new`) are checked, matching the set of
/// methods exposed as contract functions by
/// [`super::functions::public_methods`].
pub(crate) fn inherent_method_emits(impl_block: &ItemImpl) -> Vec<EventInfo> {
    impl_method_emits(impl_block, |method| {
        matches!(method.vis, Visibility::Public(_)) && method.sig.ident != "new"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalize_tokens(tokens: TokenStream2) -> String {
        tokens
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    // =========================================================================
    // EmitVisitor tests
    // =========================================================================

    #[test]
    fn test_emit_visitor_finds_emit_call() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn pause(&mut self) {
                    self.is_paused = true;
                    abi::emit("paused", PauseEvent {});
                }
            }
        };

        let mut visitor = EmitVisitor::new();
        visitor.visit_item_impl(&impl_block);

        assert_eq!(visitor.events.len(), 1);
        assert_eq!(visitor.events[0].topic, "paused");
    }

    #[test]
    fn test_emit_visitor_finds_const_topic() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn pause(&mut self) {
                    abi::emit(events::PauseToggled::PAUSED, events::PauseToggled());
                }
            }
        };

        let mut visitor = EmitVisitor::new();
        visitor.visit_item_impl(&impl_block);

        assert_eq!(visitor.events.len(), 1);
        assert_eq!(visitor.events[0].topic, "events::PauseToggled::PAUSED");
    }

    #[test]
    fn test_emit_visitor_multiple_emits() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn transfer(&mut self) {
                    abi::emit("started", StartEvent {});
                    // do work
                    abi::emit("completed", CompleteEvent {});
                }
            }
        };

        let mut visitor = EmitVisitor::new();
        visitor.visit_item_impl(&impl_block);

        assert_eq!(visitor.events.len(), 2);
    }

    #[test]
    fn test_emit_visitor_nested_in_if() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn maybe_emit(&mut self, condition: bool) {
                    if condition {
                        abi::emit("conditional", Event {});
                    }
                }
            }
        };

        let mut visitor = EmitVisitor::new();
        visitor.visit_item_impl(&impl_block);

        assert_eq!(visitor.events.len(), 1);
        assert_eq!(visitor.events[0].topic, "conditional");
    }

    #[test]
    fn test_emit_visitor_nested_in_loop() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn emit_many(&mut self, items: Vec<u32>) {
                    for item in items {
                        abi::emit("item_processed", ItemEvent { value: item });
                    }
                }
            }
        };

        let mut visitor = EmitVisitor::new();
        visitor.visit_item_impl(&impl_block);

        assert_eq!(visitor.events.len(), 1);
    }

    #[test]
    fn test_emit_visitor_just_emit_without_abi_prefix() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn do_something(&mut self) {
                    emit("event", SomeEvent {});
                }
            }
        };

        let mut visitor = EmitVisitor::new();
        visitor.visit_item_impl(&impl_block);

        assert_eq!(visitor.events.len(), 1);
        assert_eq!(visitor.events[0].topic, "event");
    }

    #[test]
    fn test_emit_visitor_no_emit_calls() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn get_value(&self) -> u64 {
                    self.value
                }
            }
        };

        let mut visitor = EmitVisitor::new();
        visitor.visit_item_impl(&impl_block);

        assert_eq!(visitor.events.len(), 0);
    }

    #[test]
    fn test_emit_visitor_across_multiple_methods() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn pause(&mut self) {
                    abi::emit("paused", PauseEvent {});
                }
                pub fn unpause(&mut self) {
                    abi::emit("unpaused", UnpauseEvent {});
                }
            }
        };

        let mut visitor = EmitVisitor::new();
        visitor.visit_item_impl(&impl_block);

        assert_eq!(visitor.events.len(), 2);
    }

    // =========================================================================
    // dedup_events_by_topic tests
    //
    // Pin the cross-source first-wins filter that the `contract` macro
    // applies after gathering events from `emit_calls`,
    // `inherent_method_emits`, and `trait_method_emits`. The same helper
    // is also reused inside `emit_calls` itself.
    // =========================================================================

    #[test]
    fn test_dedup_events_by_topic_collision_keeps_first() {
        // Two events sharing a topic but registering structurally different
        // data types. The first-seen survives; the second is dropped silently
        // (no diagnostic, no panic).
        let events = vec![
            EventInfo {
                topic: "shared_topic".to_string(),
                data_type: quote! { FirstEvent },
            },
            EventInfo {
                topic: "shared_topic".to_string(),
                data_type: quote! { SecondEvent },
            },
        ];

        let deduped = dedup_events_by_topic(events);

        assert_eq!(
            deduped.len(),
            1,
            "exactly one event survives a topic collision"
        );
        assert_eq!(deduped[0].topic, "shared_topic");
        assert_eq!(
            deduped[0].data_type.to_string(),
            "FirstEvent",
            "first-seen data type wins; the colliding entry is dropped silently"
        );
    }

    #[test]
    fn test_dedup_events_by_topic_no_overreach_for_distinct_topics() {
        // Same data type registered under two distinct topics: dedup must not
        // collapse them — only topics, not data types, drive the filter.
        let events = vec![
            EventInfo {
                topic: "topic_a".to_string(),
                data_type: quote! { SharedEvent },
            },
            EventInfo {
                topic: "topic_b".to_string(),
                data_type: quote! { SharedEvent },
            },
        ];

        let deduped = dedup_events_by_topic(events);

        assert_eq!(
            deduped.len(),
            2,
            "distinct topics survive even when data types match"
        );
        assert_eq!(deduped[0].topic, "topic_a");
        assert_eq!(deduped[1].topic, "topic_b");
    }

    #[test]
    fn test_dedup_events_by_topic_via_extract_pipeline() {
        // End-to-end through the extract layer: build an impl block where two
        // public methods carry `#[contract(emits = [...])]` attributes that
        // share a topic but supply different data types. The macro pipeline
        // (inherent_method_emits → dedup_events_by_topic) keeps the first
        // occurrence and drops the rest.
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                #[contract(emits = [(SHARED::TOPIC, FirstEvent)])]
                pub fn first(&mut self) {}

                #[contract(emits = [(SHARED::TOPIC, SecondEvent)])]
                pub fn second(&mut self) {}
            }
        };

        let collected = inherent_method_emits(&impl_block);
        assert_eq!(
            collected.len(),
            2,
            "extract layer surfaces both events before dedup"
        );

        let deduped = dedup_events_by_topic(collected);
        assert_eq!(deduped.len(), 1, "cross-source dedup keeps a single event");
        assert_eq!(deduped[0].topic, "SHARED::TOPIC");
        assert_eq!(
            deduped[0].data_type.to_string(),
            "FirstEvent",
            "first method's registration wins"
        );
    }

    // ========================================================================
    // topic_from_expr tests
    // ========================================================================

    #[test]
    fn test_topic_from_expr_string_literal() {
        let expr: Expr = syn::parse_quote!("my_topic");
        assert_eq!(topic_from_expr(&expr), Some("my_topic".to_string()));
    }

    #[test]
    fn test_topic_from_expr_const_path() {
        let expr: Expr = syn::parse_quote!(MyEvent::TOPIC);
        assert_eq!(topic_from_expr(&expr), Some("MyEvent::TOPIC".to_string()));
    }

    #[test]
    fn test_topic_from_expr_module_path() {
        let expr: Expr = syn::parse_quote!(events::MyEvent::TOPIC);
        assert_eq!(
            topic_from_expr(&expr),
            Some("events::MyEvent::TOPIC".to_string())
        );
    }

    #[test]
    fn test_topic_from_expr_variable() {
        // Variable returns the variable name (warning emitted separately)
        let expr: Expr = syn::parse_quote!(topic);
        assert_eq!(topic_from_expr(&expr), Some("topic".to_string()));
    }

    #[test]
    fn test_topic_from_expr_uppercase_single_ident() {
        // Single uppercase ident is likely a const, not a variable
        let expr: Expr = syn::parse_quote!(TOPIC);
        assert_eq!(topic_from_expr(&expr), Some("TOPIC".to_string()));
    }

    #[test]
    fn test_topic_from_expr_non_path_returns_none() {
        // Non-path expressions return None
        let expr: Expr = syn::parse_quote!(some_fn());
        assert_eq!(topic_from_expr(&expr), None);
    }

    // ========================================================================
    // emit_calls topic-collision dedup
    // ========================================================================

    #[test]
    fn test_emit_calls_dedups_topic_collision_keeps_first() {
        // Two `abi::emit` calls share a topic but supply different data types.
        // The dedup inside `emit_calls` keeps the first occurrence and drops
        // the second silently — no diagnostic, no panic.
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn first(&mut self) {
                    abi::emit("shared", FirstEvent {});
                }
                pub fn second(&mut self) {
                    abi::emit("shared", SecondEvent {});
                }
            }
        };

        let events = emit_calls(&impl_block);

        assert_eq!(
            events.len(),
            1,
            "exactly one event survives the topic collision"
        );
        assert_eq!(events[0].topic, "shared");
        assert_eq!(
            normalize_tokens(events[0].data_type.clone()),
            "FirstEvent",
            "first-seen data type wins; the colliding entry is dropped silently"
        );
    }

    #[test]
    fn test_emit_calls_preserves_distinct_topics_with_same_data_type() {
        // Same data type emitted under two distinct topics must NOT collapse —
        // dedup is keyed on topic only, never on data type.
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn alpha(&mut self) {
                    abi::emit("topic_a", SharedEvent {});
                }
                pub fn beta(&mut self) {
                    abi::emit("topic_b", SharedEvent {});
                }
            }
        };

        let events = emit_calls(&impl_block);

        assert_eq!(events.len(), 2, "distinct topics are not collapsed");
        let topics: Vec<_> = events.iter().map(|e| e.topic.as_str()).collect();
        assert_eq!(topics, vec!["topic_a", "topic_b"]);
    }

    // ========================================================================
    // trait_method_emits / inherent_method_emits tests
    // ========================================================================

    #[test]
    fn test_trait_method_emits_collects_events() {
        let impl_block: ItemImpl = syn::parse_quote! {
            #[contract(expose = [transfer_ownership])]
            impl OwnableTrait for MyContract {
                #[contract(emits = [(Transferred::TOPIC, Transferred)])]
                fn transfer_ownership(&mut self) {}

                // Not in expose list — should be ignored even with emits.
                #[contract(emits = [(Hidden::TOPIC, Hidden)])]
                fn unexposed(&mut self) {}
            }
        };
        let trait_impl = TraitImplInfo {
            trait_name: "OwnableTrait".to_string(),
            impl_block: &impl_block,
            expose_list: vec!["transfer_ownership".to_string()],
        };
        let events = trait_method_emits(&trait_impl);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic, "Transferred::TOPIC");
    }

    #[test]
    fn test_inherent_method_emits_collects_events() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                #[contract(emits = [(Resolved::TOPIC, Resolved)])]
                pub fn resolve(&mut self) { self.core.resolve(); }

                // Private method — should be ignored.
                #[contract(emits = [(Hidden::TOPIC, Hidden)])]
                fn private_helper(&mut self) { self.core.hidden(); }

                // Constructor — should be ignored even if it carries emits.
                #[contract(emits = [(New::TOPIC, New)])]
                pub fn new() -> Self { Self }
            }
        };
        let events = inherent_method_emits(&impl_block);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic, "Resolved::TOPIC");
    }
}
