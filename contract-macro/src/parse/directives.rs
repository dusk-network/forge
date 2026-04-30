// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Parsers for the `#[contract(...)]` directive on impls and methods.
//!
//! These are four ad-hoc parsers (`expose`, `emits`, `feeds`, `no_event`),
//! collected here pending consolidation into a single typed parser.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Attribute;

/// Check if method has `#[contract(no_event)]` attribute to suppress the emit
/// validation.
pub(super) fn event_suppressed(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("contract")
            && let Ok(meta) = attr.meta.require_list()
        {
            let tokens = meta.tokens.to_string();
            return tokens.contains("no_event");
        }
        false
    })
}

/// Extract the `feeds` type from a `#[contract(feeds = "Type")]` attribute.
///
/// This attribute specifies the type fed via `abi::feed()` for streaming
/// functions. When present, the data-driver uses this type for
/// `decode_output_fn` instead of the function's return type.
///
/// Returns `Some(TokenStream2)` with the feed type if found, `None` otherwise.
pub(super) fn extract_feeds_attribute(attrs: &[Attribute]) -> Option<TokenStream2> {
    for attr in attrs {
        if !attr.path().is_ident("contract") {
            continue;
        }

        let Ok(meta) = attr.meta.require_list() else {
            continue;
        };

        // Parse: feeds = "Type"
        let tokens = meta.tokens.clone();
        let mut iter = tokens.into_iter().peekable();

        // Look for "feeds"
        let Some(proc_macro2::TokenTree::Ident(ident)) = iter.next() else {
            continue;
        };
        if ident != "feeds" {
            continue;
        }

        // Expect "="
        let Some(proc_macro2::TokenTree::Punct(punct)) = iter.next() else {
            continue;
        };
        if punct.as_char() != '=' {
            continue;
        }

        // Expect string literal with type
        let Some(proc_macro2::TokenTree::Literal(lit)) = iter.next() else {
            continue;
        };
        let lit_str = lit.to_string();
        // Remove quotes from the literal
        let type_str = lit_str.trim_matches('"');

        // Parse the type string into tokens
        if let Ok(ty) = syn::parse_str::<syn::Type>(type_str) {
            return Some(quote! { #ty });
        }
    }

    None
}

/// Extract the `expose = [method1, method2, ...]` list from a
/// `#[contract(...)]` attribute.
///
/// Returns `None` if there's no `#[contract(expose = [...])]` attribute.
/// Returns `Some(vec![...])` with the method names if found.
pub(super) fn expose_list(attrs: &[Attribute]) -> Option<Vec<String>> {
    for attr in attrs {
        if !attr.path().is_ident("contract") {
            continue;
        }

        let Ok(meta) = attr.meta.require_list() else {
            continue;
        };

        // Parse: expose = [method1, method2, ...]
        let tokens = meta.tokens.clone();
        let mut iter = tokens.into_iter().peekable();

        // Look for "expose"
        let Some(proc_macro2::TokenTree::Ident(ident)) = iter.next() else {
            continue;
        };
        if ident != "expose" {
            continue;
        }

        // Expect "="
        let Some(proc_macro2::TokenTree::Punct(punct)) = iter.next() else {
            continue;
        };
        if punct.as_char() != '=' {
            continue;
        }

        // Expect "[...]"
        let Some(proc_macro2::TokenTree::Group(group)) = iter.next() else {
            continue;
        };
        if group.delimiter() != proc_macro2::Delimiter::Bracket {
            continue;
        }

        // Parse the method names from the group
        let mut methods = Vec::new();
        for token in group.stream() {
            if let proc_macro2::TokenTree::Ident(method_ident) = token {
                methods.push(method_ident.to_string());
            }
            // Skip commas and other punctuation
        }

        return Some(methods);
    }

    None
}

/// Extract the `emits = [(topic, Type), ...]` list from a `#[contract(...)]`
/// attribute.
///
/// Returns `None` if there's no `#[contract(emits = [...])]` attribute.
/// Returns `Some(vec![...])` with (topic, `data_type`) pairs if found.
///
/// Supports two topic formats:
/// - Const path: `(events::OwnershipTransferred::TOPIC,
///   events::OwnershipTransferred)`
/// - String literal: `("my_topic", MyEventType)`
pub(super) fn emits_list(attrs: &[Attribute]) -> Option<Vec<(String, TokenStream2)>> {
    for attr in attrs {
        if !attr.path().is_ident("contract") {
            continue;
        }

        let Ok(meta) = attr.meta.require_list() else {
            continue;
        };

        // Parse the token stream to find emits = [...]
        let tokens = meta.tokens.clone();
        let mut iter = tokens.into_iter().peekable();

        // Look through all tokens for "emits"
        while let Some(token) = iter.next() {
            let proc_macro2::TokenTree::Ident(ident) = token else {
                continue;
            };

            if ident != "emits" {
                continue;
            }

            // Expect "="
            let Some(proc_macro2::TokenTree::Punct(punct)) = iter.next() else {
                continue;
            };
            if punct.as_char() != '=' {
                continue;
            }

            // Expect "[...]"
            let Some(proc_macro2::TokenTree::Group(group)) = iter.next() else {
                continue;
            };
            if group.delimiter() != proc_macro2::Delimiter::Bracket {
                continue;
            }

            // Parse the event tuples from the group
            return Some(parse_emits_tuples(group.stream()));
        }
    }

    None
}

/// Parse the contents of `emits = [...]` into a list of (topic, `data_type`)
/// pairs.
fn parse_emits_tuples(stream: proc_macro2::TokenStream) -> Vec<(String, TokenStream2)> {
    let mut events = Vec::new();

    for token in stream {
        // Each event is a group: (topic, Type)
        let proc_macro2::TokenTree::Group(group) = token else {
            continue;
        };
        if group.delimiter() != proc_macro2::Delimiter::Parenthesis {
            continue;
        }

        if let Some((topic, data_type)) = parse_event_tuple(group.stream()) {
            events.push((topic, data_type));
        }
    }

    events
}

/// Parse a single event tuple: (topic, Type).
fn parse_event_tuple(stream: proc_macro2::TokenStream) -> Option<(String, TokenStream2)> {
    let mut iter = stream.into_iter().peekable();

    // Extract topic (everything before the comma)
    let topic = extract_topic_from_tokens(&mut iter)?;

    // Skip the comma
    while let Some(token) = iter.peek() {
        if let proc_macro2::TokenTree::Punct(p) = token
            && p.as_char() == ','
        {
            iter.next();
            break;
        }
        iter.next();
    }

    // Remaining tokens are the data type
    let data_type: TokenStream2 = iter.collect();
    if data_type.is_empty() {
        return None;
    }

    Some((topic, data_type))
}

/// Extract the topic string from the tokens before the comma.
fn extract_topic_from_tokens(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
) -> Option<String> {
    let mut path_segments = Vec::new();

    while let Some(token) = iter.peek() {
        match token {
            proc_macro2::TokenTree::Punct(p) if p.as_char() == ',' => {
                // End of topic
                break;
            }
            proc_macro2::TokenTree::Punct(p) if p.as_char() == ':' => {
                // Part of path separator ::
                iter.next();
            }
            proc_macro2::TokenTree::Ident(ident) => {
                path_segments.push(ident.to_string());
                iter.next();
            }
            proc_macro2::TokenTree::Literal(lit) => {
                // String literal topic
                let s = lit.to_string();
                iter.next();
                // Remove quotes from string literal
                if s.starts_with('"') && s.ends_with('"') {
                    return Some(s[1..s.len() - 1].to_string());
                }
                return Some(s);
            }
            _ => {
                iter.next();
            }
        }
    }

    if path_segments.is_empty() {
        None
    } else {
        Some(path_segments.join("::"))
    }
}

#[cfg(test)]
mod tests {
    use syn::ItemImpl;

    use super::*;

    #[test]
    fn test_expose_list_simple() {
        let impl_block: ItemImpl = syn::parse_quote! {
            #[contract(expose = [owner, transfer_ownership])]
            impl OwnableTrait for MyContract {
                fn owner(&self) -> Address { self.owner }
            }
        };
        let expose_list = expose_list(&impl_block.attrs);
        assert!(expose_list.is_some());
        let list = expose_list.unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"owner".to_string()));
        assert!(list.contains(&"transfer_ownership".to_string()));
    }

    #[test]
    fn test_expose_list_single() {
        let impl_block: ItemImpl = syn::parse_quote! {
            #[contract(expose = [version])]
            impl ISemver for MyContract {}
        };
        let expose_list = expose_list(&impl_block.attrs);
        assert!(expose_list.is_some());
        let list = expose_list.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], "version");
    }

    #[test]
    fn test_expose_list_none() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl OwnableTrait for MyContract {
                fn owner(&self) -> Address { self.owner }
            }
        };
        let expose_list = expose_list(&impl_block.attrs);
        assert!(expose_list.is_none());
    }

    #[test]
    fn test_expose_list_other_attribute() {
        let impl_block: ItemImpl = syn::parse_quote! {
            #[derive(Debug)]
            impl OwnableTrait for MyContract {
                fn owner(&self) -> Address { self.owner }
            }
        };
        let expose_list = expose_list(&impl_block.attrs);
        assert!(expose_list.is_none());
    }
}
