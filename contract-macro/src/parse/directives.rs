// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Parser for the `#[contract(...)]` directive.
//!
//! A single typed pass over the directive list yields a [`ContractDirectives`]
//! struct with one field per supported keyword (`feeds`, `expose`). Shape
//! mismatches surface as span-anchored `syn::Error`s.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Attribute, Error as SynError, Ident, LitStr, Token, Type, bracketed, parse_str, parse2, token,
};

/// Parsed `#[contract(...)]` directives, aggregated across every contract
/// attribute on a given item.
///
/// Fields default to `None` when the directive is absent.
#[derive(Default)]
pub(crate) struct ContractDirectives {
    /// Type tokens parsed from `feeds = "<TypeName>"`.
    pub feeds: Option<TokenStream2>,
    /// Method names from `expose = [m1, m2, ...]`.
    pub expose: Option<Vec<String>>,
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
    /// The keyword identifier (`feeds`, `expose`) — kept so duplicate-directive
    /// errors can span at the offending occurrence.
    keyword: Ident,
    kind: DirectiveKind,
}

enum DirectiveKind {
    Feeds(TokenStream2),
    Expose(Vec<String>),
}

impl Parse for DirectiveItem {
    fn parse(input: ParseStream) -> Result<Self, SynError> {
        let keyword: Ident = input.parse()?;
        let name = keyword.to_string();
        let kind = match name.as_str() {
            "feeds" => DirectiveKind::Feeds(parse_feeds(input, &keyword)?),
            "expose" => DirectiveKind::Expose(parse_expose(input, &keyword)?),
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

fn unknown_directive_msg(unknown: &str) -> String {
    let suggestion = match unknown {
        "exposes" => Some("expose"),
        "feed" => Some("feeds"),
        _ => None,
    };
    match suggestion {
        Some(s) => format!("unknown contract directive `{unknown}`; did you mean `{s}`?"),
        None => format!("unknown contract directive `{unknown}`; expected one of: feeds, expose"),
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
    fn parses_feeds_and_expose_in_one_attribute() {
        // Both supported directives can coexist inside a single
        // `#[contract(...)]` invocation.
        let impl_block: ItemImpl = parse_quote! {
            #[contract(feeds = "Stream", expose = [m])]
            impl Trait for MyContract {}
        };
        let d = parse_impl(&impl_block).unwrap();
        assert!(d.feeds.is_some());
        assert_eq!(d.expose.unwrap(), vec!["m".to_string()]);
    }

    #[test]
    fn aggregates_directives_across_multiple_attributes() {
        // Two `#[contract(...)]` attributes on the same item are folded
        // together into a single `ContractDirectives`.
        let impl_block: ItemImpl = parse_quote! {
            #[contract(expose = [m])]
            #[contract(feeds = "T")]
            impl Trait for MyContract {}
        };
        let d = parse_impl(&impl_block).unwrap();
        assert_eq!(d.expose.unwrap(), vec!["m".to_string()]);
        assert!(d.feeds.is_some());
    }

    #[test]
    fn err_unknown_directive() {
        let method: ImplItemFn = parse_quote! {
            #[contract(exposes)]
            fn act(&mut self) {}
        };
        let err = expect_err(parse_method(&method));
        let msg = err.to_string();
        assert!(msg.contains("unknown contract directive"), "got: {msg}");
        assert!(msg.contains("exposes"), "got: {msg}");
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
    }

    #[test]
    fn err_unknown_directive_suggests_expose_for_exposes() {
        let method: ImplItemFn = parse_quote! {
            #[contract(exposes = [m])]
            fn act(&mut self) {}
        };
        let msg = expect_err(parse_method(&method)).to_string();
        assert!(msg.contains("did you mean"), "got: {msg}");
        assert!(msg.contains("expose"), "got: {msg}");
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
    fn err_duplicate_expose_across_attributes() {
        // Duplicates across `#[contract(...)]` attributes must error too —
        // otherwise the first list is silently dropped.
        let impl_block: ItemImpl = parse_quote! {
            #[contract(expose = [a])]
            #[contract(expose = [b])]
            impl Trait for MyContract {}
        };
        let msg = expect_err(parse_impl(&impl_block)).to_string();
        assert!(msg.contains("duplicate"), "got: {msg}");
        assert!(msg.contains("expose"), "got: {msg}");
    }
}
