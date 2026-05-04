// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Parser for the `#[contract(...)]` directive.
//!
//! A single typed pass over the directive list yields a [`ContractDirectives`]
//! struct with one field per supported keyword (`feeds`, `expose`, `emits`,
//! `no_event`). Shape mismatches surface as span-anchored `syn::Error`s.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Attribute, Error as SynError, Ident, LitStr, Path, Token, Type, bracketed, parenthesized,
    parse_str, parse2, token,
};

use crate::EventInfo;

/// Parsed `#[contract(...)]` directives, aggregated across every contract
/// attribute on a given item.
///
/// Fields default to `None` / `false` when the directive is absent.
#[derive(Default)]
pub(crate) struct ContractDirectives {
    /// Type tokens parsed from `feeds = "<TypeName>"`.
    pub feeds: Option<TokenStream2>,
    /// Method names from `expose = [m1, m2, ...]`.
    pub expose: Option<Vec<String>>,
    /// Event metadata from `emits = [(topic, EventType), ...]`.
    pub emits: Option<Vec<EventInfo>>,
    /// Set when `no_event` appears.
    pub no_event: bool,
}

/// Parse all `#[contract(...)]` attributes on an item into a single typed
/// struct.
///
/// Unrelated attributes (e.g. `#[doc]`, `#[cfg]`) are ignored. Malformed
/// directive shapes (typo'd keyword, missing value, wrong delimiter) and
/// duplicate directives (whether within one attribute or across multiple)
/// return a `syn::Error` whose span points at the offending token.
pub(crate) fn parse_contract_directives(
    attrs: &[Attribute],
) -> Result<ContractDirectives, SynError> {
    let mut out = ContractDirectives::default();

    for attr in attrs {
        if !attr.path().is_ident("contract") {
            continue;
        }

        let list = attr.meta.require_list()?;
        let parsed: DirectiveList = parse2(list.tokens.clone())?;
        for item in parsed.items {
            apply_directive(&mut out, item)?;
        }
    }

    Ok(out)
}

fn apply_directive(out: &mut ContractDirectives, item: DirectiveItem) -> Result<(), SynError> {
    let DirectiveItem { keyword, kind } = item;
    match kind {
        DirectiveKind::Feeds(ts) => set_once(&mut out.feeds, ts, &keyword),
        DirectiveKind::Expose(names) => set_once(&mut out.expose, names, &keyword),
        DirectiveKind::Emits(events) => set_once(&mut out.emits, events, &keyword),
        DirectiveKind::NoEvent => {
            if out.no_event {
                return Err(duplicate_directive(&keyword));
            }
            out.no_event = true;
            Ok(())
        }
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T, keyword: &Ident) -> Result<(), SynError> {
    if slot.is_some() {
        return Err(duplicate_directive(keyword));
    }
    *slot = Some(value);
    Ok(())
}

fn duplicate_directive(keyword: &Ident) -> SynError {
    SynError::new(keyword.span(), format!("duplicate `{keyword}` directive"))
}

struct DirectiveList {
    items: Vec<DirectiveItem>,
}

impl Parse for DirectiveList {
    fn parse(input: ParseStream) -> Result<Self, SynError> {
        let items = Punctuated::<DirectiveItem, Token![,]>::parse_terminated(input)?
            .into_iter()
            .collect();
        Ok(Self { items })
    }
}

struct DirectiveItem {
    /// The keyword identifier (`feeds`, `expose`, `emits`, `no_event`) — kept
    /// so duplicate-directive errors can span at the offending occurrence.
    keyword: Ident,
    kind: DirectiveKind,
}

enum DirectiveKind {
    Feeds(TokenStream2),
    Expose(Vec<String>),
    Emits(Vec<EventInfo>),
    NoEvent,
}

impl Parse for DirectiveItem {
    fn parse(input: ParseStream) -> Result<Self, SynError> {
        let keyword: Ident = input.parse()?;
        let name = keyword.to_string();
        let kind = match name.as_str() {
            "no_event" => DirectiveKind::NoEvent,
            "feeds" => DirectiveKind::Feeds(parse_feeds(input, &keyword)?),
            "expose" => DirectiveKind::Expose(parse_expose(input, &keyword)?),
            "emits" => DirectiveKind::Emits(parse_emits(input, &keyword)?),
            unknown => {
                return Err(SynError::new(
                    keyword.span(),
                    unknown_directive_msg(unknown),
                ));
            }
        };
        Ok(Self { keyword, kind })
    }
}

fn parse_feeds(input: ParseStream, kw: &Ident) -> Result<TokenStream2, SynError> {
    // For a missing `=`, the most informative span is the keyword; for a
    // wrong-shape value (`feeds = SomeType`), syn's parse failure already
    // points at the offending token — we just rewrite the message so the
    // user sees the directive's expected shape.
    let expected = "expected `feeds = \"<TypeName>\"`";
    input
        .parse::<Token![=]>()
        .map_err(|_| SynError::new(kw.span(), expected))?;
    let lit: LitStr = input
        .parse()
        .map_err(|err| SynError::new(err.span(), expected))?;
    let ty: Type = parse_str(&lit.value()).map_err(|_| {
        SynError::new(
            lit.span(),
            format!("feeds type `{}` is not a valid Rust type", lit.value()),
        )
    })?;
    Ok(quote! { #ty })
}

fn parse_expose(input: ParseStream, kw: &Ident) -> Result<Vec<String>, SynError> {
    let expected = "expected `expose = [m1, m2, ...]`";
    input
        .parse::<Token![=]>()
        .map_err(|_| SynError::new(kw.span(), expected))?;
    if !input.peek(token::Bracket) {
        return Err(SynError::new(input.span(), expected));
    }
    let content;
    bracketed!(content in input);
    let names = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?
        .into_iter()
        .map(|i| i.to_string())
        .collect();
    Ok(names)
}

fn parse_emits(input: ParseStream, kw: &Ident) -> Result<Vec<EventInfo>, SynError> {
    let expected = "expected `emits = [(topic, EventType), ...]`";
    input
        .parse::<Token![=]>()
        .map_err(|_| SynError::new(kw.span(), expected))?;
    if !input.peek(token::Bracket) {
        return Err(SynError::new(input.span(), expected));
    }
    let content;
    bracketed!(content in input);
    let tuples = Punctuated::<EventTuple, Token![,]>::parse_terminated(&content)?;
    Ok(tuples
        .into_iter()
        .map(|t| EventInfo {
            topic: t.topic,
            data_type: t.data_type,
        })
        .collect())
}

struct EventTuple {
    topic: String,
    data_type: TokenStream2,
}

impl Parse for EventTuple {
    fn parse(input: ParseStream) -> Result<Self, SynError> {
        let content;
        parenthesized!(content in input);
        let topic = if content.peek(LitStr) {
            let lit: LitStr = content.parse()?;
            lit.value()
        } else {
            let path: Path = content.parse()?;
            path.segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::")
        };
        content.parse::<Token![,]>()?;
        let ty: Type = content.parse()?;
        if !content.is_empty() {
            return Err(content.error("expected `(topic, EventType)`"));
        }
        Ok(Self {
            topic,
            data_type: quote! { #ty },
        })
    }
}

fn unknown_directive_msg(unknown: &str) -> String {
    let suggestion = match unknown {
        "emit" => Some("emits"),
        "no_events" => Some("no_event"),
        "exposes" => Some("expose"),
        "feed" => Some("feeds"),
        _ => None,
    };
    match suggestion {
        Some(s) => format!("unknown contract directive `{unknown}`; did you mean `{s}`?"),
        None => format!(
            "unknown contract directive `{unknown}`; expected one of: feeds, expose, emits, no_event"
        ),
    }
}

#[cfg(test)]
mod tests {
    use syn::{ImplItemFn, ItemImpl, parse_quote};

    use super::*;

    fn parse_impl(impl_block: &ItemImpl) -> Result<ContractDirectives, SynError> {
        parse_contract_directives(&impl_block.attrs)
    }

    fn parse_method(method: &ImplItemFn) -> Result<ContractDirectives, SynError> {
        parse_contract_directives(&method.attrs)
    }

    fn expect_err<T>(result: Result<T, SynError>) -> SynError {
        match result {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        }
    }

    #[test]
    fn empty_attrs_yield_default() {
        let impl_block: ItemImpl = parse_quote! {
            impl Foo for MyContract {}
        };
        let d = parse_impl(&impl_block).unwrap();
        assert!(d.feeds.is_none());
        assert!(d.expose.is_none());
        assert!(d.emits.is_none());
        assert!(!d.no_event);
    }

    #[test]
    fn unrelated_attribute_passes_through() {
        let method: ImplItemFn = parse_quote! {
            #[doc = "hello"]
            #[allow(dead_code)]
            #[cfg(test)]
            fn foo(&self) {}
        };
        let d = parse_method(&method).unwrap();
        assert!(d.feeds.is_none());
        assert!(d.expose.is_none());
        assert!(d.emits.is_none());
        assert!(!d.no_event);
    }

    #[test]
    fn parses_feeds() {
        let method: ImplItemFn = parse_quote! {
            #[contract(feeds = "MyType")]
            fn stream(&self) {}
        };
        let d = parse_method(&method).unwrap();
        let feeds = d.feeds.expect("feeds field set");
        assert_eq!(feeds.to_string().replace(' ', ""), "MyType");
    }

    #[test]
    fn parses_feeds_generic_type() {
        let method: ImplItemFn = parse_quote! {
            #[contract(feeds = "Vec<u8>")]
            fn stream(&self) {}
        };
        let d = parse_method(&method).unwrap();
        let feeds = d.feeds.expect("feeds field set");
        assert_eq!(feeds.to_string().replace(' ', ""), "Vec<u8>");
    }

    #[test]
    fn parses_expose() {
        let impl_block: ItemImpl = parse_quote! {
            #[contract(expose = [owner, transfer_ownership])]
            impl OwnableTrait for MyContract {}
        };
        let d = parse_impl(&impl_block).unwrap();
        let expose = d.expose.expect("expose field set");
        assert_eq!(
            expose,
            vec!["owner".to_string(), "transfer_ownership".to_string()]
        );
    }

    #[test]
    fn parses_expose_single_method() {
        let impl_block: ItemImpl = parse_quote! {
            #[contract(expose = [version])]
            impl ISemver for MyContract {}
        };
        let d = parse_impl(&impl_block).unwrap();
        assert_eq!(d.expose.unwrap(), vec!["version".to_string()]);
    }

    #[test]
    fn parses_emits_path_topic() {
        let method: ImplItemFn = parse_quote! {
            #[contract(emits = [(events::OwnershipTransferred::TOPIC, events::OwnershipTransferred)])]
            fn transfer(&mut self) {}
        };
        let d = parse_method(&method).unwrap();
        let emits = d.emits.expect("emits field set");
        assert_eq!(emits.len(), 1);
        assert_eq!(emits[0].topic, "events::OwnershipTransferred::TOPIC");
        assert_eq!(
            emits[0].data_type.to_string().replace(' ', ""),
            "events::OwnershipTransferred"
        );
    }

    #[test]
    fn parses_emits_string_topic() {
        let method: ImplItemFn = parse_quote! {
            #[contract(emits = [("custom_topic", MyEvent)])]
            fn transfer(&mut self) {}
        };
        let d = parse_method(&method).unwrap();
        let emits = d.emits.expect("emits field set");
        assert_eq!(emits.len(), 1);
        assert_eq!(emits[0].topic, "custom_topic");
    }

    #[test]
    fn parses_emits_multiple() {
        let method: ImplItemFn = parse_quote! {
            #[contract(emits = [
                (events::A::TOPIC, events::A),
                ("b_topic", B),
            ])]
            fn act(&mut self) {}
        };
        let d = parse_method(&method).unwrap();
        let emits = d.emits.unwrap();
        assert_eq!(emits.len(), 2);
        assert_eq!(emits[0].topic, "events::A::TOPIC");
        assert_eq!(emits[1].topic, "b_topic");
    }

    #[test]
    fn parses_no_event() {
        let method: ImplItemFn = parse_quote! {
            #[contract(no_event)]
            fn touch(&mut self) {}
        };
        let d = parse_method(&method).unwrap();
        assert!(d.no_event);
    }

    #[test]
    fn parses_combined_directives() {
        let method: ImplItemFn = parse_quote! {
            #[contract(feeds = "Stream", emits = [("t", E)], no_event)]
            fn act(&mut self) {}
        };
        let d = parse_method(&method).unwrap();
        assert!(d.feeds.is_some());
        assert_eq!(d.emits.unwrap().len(), 1);
        assert!(d.no_event);
    }

    #[test]
    fn parses_feeds_in_any_position() {
        // `feeds` must be recognised regardless of position within the
        // directive list.
        let method: ImplItemFn = parse_quote! {
            #[contract(no_event, feeds = "T")]
            fn act(&mut self) {}
        };
        let d = parse_method(&method).unwrap();
        assert!(d.no_event);
        assert!(d.feeds.is_some());
    }

    #[test]
    fn feeds_value_containing_no_event_does_not_suppress() {
        // `no_event` is only set when it appears as a directive keyword,
        // never when the literal text "no_event" appears inside another
        // directive's value.
        let method: ImplItemFn = parse_quote! {
            #[contract(feeds = "Foo_no_event")]
            fn act(&self) {}
        };
        let d = parse_method(&method).unwrap();
        assert!(!d.no_event);
        assert!(d.feeds.is_some());
    }

    #[test]
    fn err_unknown_directive() {
        let method: ImplItemFn = parse_quote! {
            #[contract(no_events)]
            fn act(&mut self) {}
        };
        let err = expect_err(parse_method(&method));
        let msg = err.to_string();
        assert!(msg.contains("unknown contract directive"), "got: {msg}");
        assert!(msg.contains("no_events"), "got: {msg}");
    }

    #[test]
    fn err_unknown_directive_lists_valid_keywords() {
        let method: ImplItemFn = parse_quote! {
            #[contract(bogus = 1)]
            fn act(&self) {}
        };
        let msg = expect_err(parse_method(&method)).to_string();
        assert!(msg.contains("feeds"), "got: {msg}");
        assert!(msg.contains("expose"), "got: {msg}");
        assert!(msg.contains("emits"), "got: {msg}");
        assert!(msg.contains("no_event"), "got: {msg}");
    }

    #[test]
    fn err_unknown_directive_suggests_emits_for_emit() {
        let method: ImplItemFn = parse_quote! {
            #[contract(emit = [("t", E)])]
            fn act(&mut self) {}
        };
        let msg = expect_err(parse_method(&method)).to_string();
        assert!(msg.contains("did you mean"), "got: {msg}");
        assert!(msg.contains("emits"), "got: {msg}");
    }

    #[test]
    fn err_feeds_without_value() {
        let method: ImplItemFn = parse_quote! {
            #[contract(feeds)]
            fn act(&self) {}
        };
        let msg = expect_err(parse_method(&method)).to_string();
        assert!(msg.contains("feeds"), "got: {msg}");
        assert!(msg.contains("TypeName"), "got: {msg}");
    }

    #[test]
    fn err_feeds_non_string_value() {
        let method: ImplItemFn = parse_quote! {
            #[contract(feeds = SomeType)]
            fn act(&self) {}
        };
        let msg = expect_err(parse_method(&method)).to_string();
        assert!(msg.contains("feeds"), "got: {msg}");
        assert!(msg.contains("TypeName"), "got: {msg}");
    }

    #[test]
    fn err_feeds_unparseable_type() {
        let method: ImplItemFn = parse_quote! {
            #[contract(feeds = "fn(")]
            fn act(&self) {}
        };
        let msg = expect_err(parse_method(&method)).to_string();
        assert!(
            msg.contains("not a valid Rust type"),
            "expected type-validation error, got: {msg}"
        );
    }

    #[test]
    fn err_expose_with_parens() {
        let impl_block: ItemImpl = parse_quote! {
            #[contract(expose = (owner, version))]
            impl Foo for MyContract {}
        };
        let msg = expect_err(parse_impl(&impl_block)).to_string();
        assert!(msg.contains("expose"), "got: {msg}");
        assert!(msg.contains("[m1"), "got: {msg}");
    }

    #[test]
    fn err_emits_malformed_tuple() {
        let method: ImplItemFn = parse_quote! {
            #[contract(emits = [(BadShape)])]
            fn act(&mut self) {}
        };
        // Missing comma inside the tuple — parser should error on the
        // expected `,` in the tuple.
        let err = expect_err(parse_method(&method));
        // We only require some error; the specific message comes from
        // syn's tuple parsing. Sanity-check it's not silently dropped.
        let msg = err.to_string();
        assert!(!msg.is_empty(), "expected error for malformed tuple");
    }

    #[test]
    fn err_emits_tuple_extra_tokens() {
        // The tuple is `(topic, EventType)` — anything trailing must be
        // rejected, not silently dropped.
        let method: ImplItemFn = parse_quote! {
            #[contract(emits = [("topic", EventType, Extra)])]
            fn act(&mut self) {}
        };
        let msg = expect_err(parse_method(&method)).to_string();
        assert!(
            msg.contains("topic, EventType"),
            "error should describe the expected tuple shape, got: {msg}"
        );
    }

    #[test]
    fn err_duplicate_feeds_within_one_attribute() {
        let method: ImplItemFn = parse_quote! {
            #[contract(feeds = "A", feeds = "B")]
            fn act(&self) {}
        };
        let msg = expect_err(parse_method(&method)).to_string();
        assert!(msg.contains("duplicate"), "got: {msg}");
        assert!(msg.contains("feeds"), "got: {msg}");
    }

    #[test]
    fn err_duplicate_emits_across_attributes() {
        // Duplicates across `#[contract(...)]` attributes must error too —
        // otherwise the first list is silently dropped.
        let method: ImplItemFn = parse_quote! {
            #[contract(emits = [("a", A)])]
            #[contract(emits = [("b", B)])]
            fn act(&mut self) {}
        };
        let msg = expect_err(parse_method(&method)).to_string();
        assert!(msg.contains("duplicate"), "got: {msg}");
        assert!(msg.contains("emits"), "got: {msg}");
    }

    #[test]
    fn err_duplicate_no_event() {
        let method: ImplItemFn = parse_quote! {
            #[contract(no_event, no_event)]
            fn act(&mut self) {}
        };
        let msg = expect_err(parse_method(&method)).to_string();
        assert!(msg.contains("duplicate"), "got: {msg}");
        assert!(msg.contains("no_event"), "got: {msg}");
    }
}
