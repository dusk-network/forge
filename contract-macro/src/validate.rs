// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Validation functions for contract macro.

use quote::ToTokens;
use syn::visit::Visit;
use syn::{FnArg, ImplItem, ImplItemFn, ItemImpl, Lifetime, ReturnType, Type, Visibility};

use crate::data_driver::{handler_signature, handler_signature_display, pretty_tokens, role_name};
use crate::{CustomDataDriverHandler, ImportInfo, resolve};

/// Validate that a public method has a supported signature for extern wrapper
/// generation.
///
/// Returns an error if the method:
/// - Has generic type or const parameters
/// - Is async
/// - Consumes `self` (not `&self` or `&mut self`)
/// - Uses `impl Trait` in parameters or return type
pub(crate) fn public_method(method: &ImplItemFn) -> Result<(), syn::Error> {
    let name = &method.sig.ident;

    // Check for generic type or const parameters
    if !method.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &method.sig.generics,
            format!(
                "public method `{name}` cannot have generic or const parameters; \
                 extern \"C\" wrappers require concrete types"
            ),
        ));
    }

    // Check for async
    if method.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            method.sig.asyncness,
            format!(
                "public method `{name}` cannot be async; \
                 WASM contracts do not support async execution"
            ),
        ));
    }

    // Check for impl Trait in parameters
    for arg in &method.sig.inputs {
        if let FnArg::Typed(pat_type) = arg
            && let Type::ImplTrait(_) = &*pat_type.ty
        {
            return Err(syn::Error::new_spanned(
                &pat_type.ty,
                format!(
                    "public method `{name}` cannot use `impl Trait` in parameters; \
                     extern \"C\" wrappers require concrete types"
                ),
            ));
        }
    }

    // Check for impl Trait in return type
    if let ReturnType::Type(_, ty) = &method.sig.output
        && let Type::ImplTrait(_) = &**ty
    {
        return Err(syn::Error::new_spanned(
            ty,
            format!(
                "public method `{name}` cannot use `impl Trait` as return type; \
                 extern \"C\" wrappers require concrete types"
            ),
        ));
    }

    // Check for self receiver: if present, must be borrowed (not consumed)
    if let Some(FnArg::Receiver(receiver)) = method.sig.inputs.first()
        && receiver.reference.is_none()
    {
        return Err(syn::Error::new_spanned(
            receiver,
            format!(
                "public method `{name}` cannot consume `self`; \
                 use `&self` or `&mut self` instead"
            ),
        ));
    }

    Ok(())
}

/// Validate all public methods in an impl block.
///
/// Note: The `new` method is skipped because it's a special constructor
/// that is validated separately by `new_constructor` and is not
/// exported as an extern function.
pub(crate) fn impl_block_methods(impl_block: &ItemImpl) -> Result<(), syn::Error> {
    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item
            && matches!(method.vis, Visibility::Public(_))
            && method.sig.ident != "new"
        {
            public_method(method)?;
        }
    }
    Ok(())
}

/// Validate that the contract struct has a `const fn new() -> Self` method.
///
/// This method is required to initialize the static `STATE` variable.
/// It must be:
/// - Named `new`
/// - Marked `const`
/// - Have no parameters
/// - Return `Self` (or the contract type name)
pub(crate) fn new_constructor(
    contract_name: &str,
    impl_blocks: &[&ItemImpl],
    contract_struct: &syn::ItemStruct,
) -> Result<(), syn::Error> {
    // Find the `new` method in any impl block
    let new_method = impl_blocks.iter().find_map(|impl_block| {
        impl_block.items.iter().find_map(|item| {
            if let ImplItem::Fn(method) = item
                && method.sig.ident == "new"
            {
                Some(method)
            } else {
                None
            }
        })
    });

    let Some(new_method) = new_method else {
        return Err(syn::Error::new_spanned(
            contract_struct,
            format!(
                "#[contract] requires `{contract_name}` to have a `const fn new() -> Self` method \
                 to initialize the static STATE variable"
            ),
        ));
    };

    // Must be const
    if new_method.sig.constness.is_none() {
        return Err(syn::Error::new_spanned(
            &new_method.sig,
            format!(
                "`{contract_name}::new` must be a `const fn` to initialize the static STATE variable; \
                 add `const` to the function signature"
            ),
        ));
    }

    // Must have no parameters (no self, no other args)
    if !new_method.sig.inputs.is_empty() {
        return Err(syn::Error::new_spanned(
            &new_method.sig.inputs,
            format!(
                "`{contract_name}::new` must have no parameters; \
                 use `const fn new() -> Self` to create a default state"
            ),
        ));
    }

    // Must return Self or the contract type
    let has_valid_return = match &new_method.sig.output {
        ReturnType::Default => false,
        ReturnType::Type(_, ty) => {
            // Check for `Self`
            if let Type::Path(type_path) = &**ty {
                type_path.path.is_ident("Self") || type_path.path.is_ident(contract_name)
            } else {
                false
            }
        }
    };

    if !has_valid_return {
        return Err(syn::Error::new_spanned(
            &new_method.sig.output,
            format!("`{contract_name}::new` must return `Self` or `{contract_name}`"),
        ));
    }

    Ok(())
}

/// Validate the `init` method if present.
///
/// The `init` method is optional but if present, it must:
/// - Take `&mut self` (initialization modifies state)
/// - Return `()` (errors should panic, not return)
pub(crate) fn init_method(
    contract_name: &str,
    impl_blocks: &[&ItemImpl],
) -> Result<(), syn::Error> {
    // Find the `init` method in any impl block
    let init_method = impl_blocks.iter().find_map(|impl_block| {
        impl_block.items.iter().find_map(|item| {
            if let ImplItem::Fn(method) = item
                && method.sig.ident == "init"
            {
                Some(method)
            } else {
                None
            }
        })
    });

    // If no init method, that's fine - it's optional
    let Some(init_method) = init_method else {
        return Ok(());
    };

    // Check that it has a receiver
    let receiver = init_method.sig.inputs.first().and_then(|arg| {
        if let FnArg::Receiver(r) = arg {
            Some(r)
        } else {
            None
        }
    });

    let Some(receiver) = receiver else {
        return Err(syn::Error::new_spanned(
            &init_method.sig,
            format!(
                "`{contract_name}::init` must take `&mut self`; \
                 initialization requires access to contract state"
            ),
        ));
    };

    // Must be &mut self, not &self or self
    if receiver.reference.is_none() || receiver.mutability.is_none() {
        return Err(syn::Error::new_spanned(
            receiver,
            format!(
                "`{contract_name}::init` must take `&mut self`; \
                 initialization needs to modify contract state"
            ),
        ));
    }

    // Must return () - check for default return or explicit ()
    let returns_unit = match &init_method.sig.output {
        ReturnType::Default => true,
        ReturnType::Type(_, ty) => {
            if let Type::Tuple(tuple) = &**ty {
                tuple.elems.is_empty()
            } else {
                false
            }
        }
    };

    if !returns_unit {
        return Err(syn::Error::new_spanned(
            &init_method.sig.output,
            format!(
                "`{contract_name}::init` must return `()`; \
                 use `panic!` or `assert!` for initialization errors"
            ),
        ));
    }

    Ok(())
}

/// Validate a method from a trait impl block.
///
/// Similar to `public_method` but with trait-specific error messages.
/// For default implementations (empty body), associated functions (no self) are
/// allowed.
pub(crate) fn trait_method(
    method: &ImplItemFn,
    trait_name: &str,
    is_default_impl: bool,
) -> Result<(), syn::Error> {
    let name = &method.sig.ident;

    // Check for generic type or const parameters
    if !method.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &method.sig.generics,
            format!(
                "trait method `{trait_name}::{name}` cannot have generic or const parameters; \
                 extern \"C\" wrappers require concrete types"
            ),
        ));
    }

    // Check for async
    if method.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            method.sig.asyncness,
            format!(
                "trait method `{trait_name}::{name}` cannot be async; \
                 WASM contracts do not support async execution"
            ),
        ));
    }

    // Check for impl Trait in parameters
    for arg in &method.sig.inputs {
        if let FnArg::Typed(pat_type) = arg
            && let Type::ImplTrait(_) = &*pat_type.ty
        {
            return Err(syn::Error::new_spanned(
                &pat_type.ty,
                format!(
                    "trait method `{trait_name}::{name}` cannot use `impl Trait` in parameters; \
                     extern \"C\" wrappers require concrete types"
                ),
            ));
        }
    }

    // Check for impl Trait in return type
    if let ReturnType::Type(_, ty) = &method.sig.output
        && let Type::ImplTrait(_) = &**ty
    {
        return Err(syn::Error::new_spanned(
            ty,
            format!(
                "trait method `{trait_name}::{name}` cannot use `impl Trait` as return type; \
                 extern \"C\" wrappers require concrete types"
            ),
        ));
    }

    // Check for self receiver
    let receiver = method.sig.inputs.first().and_then(|arg| {
        if let FnArg::Receiver(r) = arg {
            Some(r)
        } else {
            None
        }
    });

    // Associated functions (no self) are allowed for default implementations
    if let Some(receiver) = receiver {
        // Check that self is borrowed, not consumed
        if receiver.reference.is_none() {
            return Err(syn::Error::new_spanned(
                receiver,
                format!(
                    "trait method `{trait_name}::{name}` cannot consume `self`; \
                     use `&self` or `&mut self` instead"
                ),
            ));
        }
    } else if !is_default_impl {
        // Non-default implementations must have self
        return Err(syn::Error::new_spanned(
            &method.sig,
            format!(
                "trait method `{trait_name}::{name}` must have a `self` receiver; \
                 for associated functions, use an empty body `{{}}` to expose the default impl"
            ),
        ));
    }

    Ok(())
}

/// Validate a custom data-driver handler's signature.
///
/// Handler functions registered via `#[contract(encode_input = "…")]`,
/// `#[contract(decode_input = "…")]`, or `#[contract(decode_output = "…")]`
/// are moved into the generated `data_driver` module and called directly by
/// the dispatch match arms. If the signature doesn't match what the dispatch
/// site expects, the downstream call site in macro-generated code fails with
/// a cryptic type error against code the user didn't write.
///
/// This validation emits a clear `compile_error!` at the handler definition
/// naming the handler, the role, and the expected signature.
///
/// Comparison pipeline:
/// - reject `'static` lifetimes outright — the generated dispatcher calls the
///   handler with a local-lifetime borrow and can't satisfy a `'static` one;
/// - run the user's argument / return types through [`resolve::resolve_type`]
///   so short-path idioms like `Vec<u8>` or `Error` (after a `use`) are
///   rewritten to their canonical fully-qualified form, then token-compare
///   against the role's expected signature. The import map is the single source
///   of truth for path equivalence — the same map that drives handler splicing
///   drives validation.
///
/// Other reference lifetimes (elided or handler-generic via `fn<'a>(…)`) are
/// accepted: the resolver strips reference lifetimes, so they're irrelevant
/// after canonicalisation.
///
/// The canonical per-role signature lives in `data_driver::handler_signature`
/// so the validator and the code that calls handlers can't drift apart.
pub(crate) fn custom_handler(
    handler: &CustomDataDriverHandler,
    imports: &[ImportInfo],
) -> Result<(), syn::Error> {
    let role = handler.role;
    let role_str = role_name(role);
    let expected = handler_signature(role);
    let expected_display = handler_signature_display(role);
    let sig = &handler.func.sig;
    let handler_name = &sig.ident;

    // Handlers are free functions, not methods.
    if let Some(FnArg::Receiver(receiver)) = sig.inputs.first() {
        return Err(syn::Error::new_spanned(
            receiver,
            format!(
                "handler `{handler_name}` for `{role_str}` must be a free function, \
                 not a method; expected signature: `{expected_display}`"
            ),
        ));
    }

    // Exactly one argument.
    let typed_args: Vec<&syn::PatType> = sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pat_type) => Some(pat_type),
            FnArg::Receiver(_) => None,
        })
        .collect();

    if typed_args.len() != 1 {
        return Err(syn::Error::new_spanned(
            &sig.inputs,
            format!(
                "handler `{handler_name}` for `{role_str}` must take exactly one \
                 argument, got {}; expected signature: `{expected_display}`",
                typed_args.len()
            ),
        ));
    }

    let arg_ty = &typed_args[0].ty;

    // A handler that demands a `'static` borrow can't be called by the
    // dispatcher — the input is a local borrow of the incoming bytes. Reject
    // before canonicalisation so the user sees a lifetime-specific message,
    // not a confusing "argument type doesn't match" after the resolver has
    // silently stripped lifetimes from the expected form.
    reject_static_lifetime(arg_ty, handler_name, role_str, &expected_display)?;

    // Canonicalise the user-written argument type through the import map and
    // token-compare to the role's canonical form.
    let resolved_arg = resolve::resolve_type(arg_ty, imports);
    if !tokens_equal(&resolved_arg, &expected.arg_type.to_string()) {
        let got_arg = pretty_tokens(&arg_ty.to_token_stream());
        let want_arg = pretty_tokens(&expected.arg_type);
        return Err(syn::Error::new_spanned(
            arg_ty,
            format!(
                "handler `{handler_name}` for `{role_str}` has argument type \
                 `{got_arg}`, expected `{want_arg}`; full expected signature: \
                 `{expected_display}`"
            ),
        ));
    }

    // Return type must match the role's canonical return type.
    let got_ret = match &sig.output {
        ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                sig,
                format!(
                    "handler `{handler_name}` for `{role_str}` must return a \
                     `Result`; expected signature: `{expected_display}`"
                ),
            ));
        }
        ReturnType::Type(_, ty) => ty,
    };

    reject_static_lifetime(got_ret, handler_name, role_str, &expected_display)?;

    let resolved_ret = resolve::resolve_type(got_ret, imports);
    if !tokens_equal(&resolved_ret, &expected.return_type.to_string()) {
        let got_ret_str = pretty_tokens(&got_ret.to_token_stream());
        let want_ret = pretty_tokens(&expected.return_type);
        return Err(syn::Error::new_spanned(
            &sig.output,
            format!(
                "handler `{handler_name}` for `{role_str}` has return type \
                 `{got_ret_str}`, expected `{want_ret}`; full expected signature: \
                 `{expected_display}`"
            ),
        ));
    }

    Ok(())
}

/// Reject any `'static` lifetime appearing in the handler's signature.
///
/// The generated dispatcher passes a local-lifetime borrow and cannot
/// satisfy a `'static` lifetime the handler promises to receive or return.
/// We check both arguments and return type; a clear message at the signature
/// site beats a mysterious lifetime mismatch deep inside macro-generated code.
fn reject_static_lifetime(
    ty: &Type,
    handler_name: &syn::Ident,
    role_str: &str,
    expected_display: &str,
) -> Result<(), syn::Error> {
    let mut finder = StaticLifetimeFinder::default();
    finder.visit_type(ty);
    if let Some(span_lifetime) = finder.first {
        return Err(syn::Error::new_spanned(
            span_lifetime,
            format!(
                "handler `{handler_name}` for `{role_str}` cannot bind a `'static` \
                 lifetime; the dispatcher passes a local borrow. Drop the lifetime \
                 or declare a handler-generic one (e.g. `fn {handler_name}<'a>(…)`). \
                 Expected signature: `{expected_display}`"
            ),
        ));
    }
    Ok(())
}

#[derive(Default)]
struct StaticLifetimeFinder {
    first: Option<Lifetime>,
}

impl<'ast> Visit<'ast> for StaticLifetimeFinder {
    fn visit_lifetime(&mut self, lt: &'ast Lifetime) {
        if self.first.is_none() && lt.ident == "static" {
            self.first = Some(lt.clone());
        }
    }
}

/// Whitespace-insensitive token-string equality.
///
/// `quote!`-produced token streams stringify with spaces around punctuation
/// (`& str`, `Result < T , E >`), while [`resolve::resolve_type`] emits
/// whitespace-free output (`&str`, `Result<T, E>`). Both forms are
/// token-identical after whitespace is stripped — use that as the comparator
/// so the two sources of truth can coexist without re-parsing either side.
fn tokens_equal(a: &str, b: &str) -> bool {
    let strip = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
    strip(a) == strip(b)
}

/// Validate that a mutating method emits events.
///
/// Public `&mut self` methods should emit events for observability. This
/// validation produces a compile error if such a method doesn't emit events,
/// unless:
/// - The method has `#[contract(no_event)]` to explicitly suppress the check
/// - The method has manual events registered via `#[contract(emits = [...])]`
pub(crate) fn method_emits_event(
    method: &ImplItemFn,
    has_emit_call: bool,
    suppressed: bool,
    has_manual_events: bool,
) -> Result<(), syn::Error> {
    // Skip if method has #[contract(no_event)] attribute
    if suppressed {
        return Ok(());
    }

    // Skip if not a mutating method (&mut self)
    let receiver = method.sig.inputs.first().and_then(|arg| {
        if let FnArg::Receiver(r) = arg {
            Some(r)
        } else {
            None
        }
    });

    let Some(receiver) = receiver else {
        // No self parameter - not a mutating method
        return Ok(());
    };

    // Only check &mut self methods
    if receiver.reference.is_none() || receiver.mutability.is_none() {
        return Ok(());
    }

    // Check if method emits events (either directly or via manual registration)
    if !has_emit_call && !has_manual_events {
        return Err(syn::Error::new_spanned(
            &method.sig,
            format!(
                "public method `{}` mutates state but emits no events; \
                 add an `abi::emit()` call or suppress with `#[contract(no_event)]`",
                method.sig.ident
            ),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_method_valid_ref_self() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn get_value(&self) -> u64 { 0 }
        };
        assert!(public_method(&method).is_ok());
    }

    #[test]
    fn test_validate_method_valid_mut_self() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn set_value(&mut self, value: u64) { }
        };
        assert!(public_method(&method).is_ok());
    }

    #[test]
    fn test_validate_method_no_self() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn empty_address() -> Address { Address::default() }
        };
        assert!(public_method(&method).is_ok());
    }

    #[test]
    fn test_validate_method_consuming_self() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn destroy(self) { }
        };
        let err = public_method(&method).unwrap_err();
        assert!(err.to_string().contains("cannot consume `self`"));
    }

    #[test]
    fn test_validate_method_generic() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn process<T>(&self, value: T) -> T { value }
        };
        let err = public_method(&method).unwrap_err();
        assert!(
            err.to_string()
                .contains("cannot have generic or const parameters")
        );
    }

    #[test]
    fn test_validate_method_async() {
        let method: ImplItemFn = syn::parse_quote! {
            pub async fn fetch_data(&self) -> u64 { 0 }
        };
        let err = public_method(&method).unwrap_err();
        assert!(err.to_string().contains("cannot be async"));
    }

    #[test]
    fn test_validate_method_const_generic() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn process<const N: usize>(&self) -> [u8; N] { [0; N] }
        };
        let err = public_method(&method).unwrap_err();
        assert!(
            err.to_string()
                .contains("cannot have generic or const parameters")
        );
    }

    #[test]
    fn test_validate_method_impl_trait_param() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn process(&self, x: impl Display) {}
        };
        let err = public_method(&method).unwrap_err();
        assert!(
            err.to_string()
                .contains("cannot use `impl Trait` in parameters")
        );
    }

    #[test]
    fn test_validate_method_impl_trait_return() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn iter(&self) -> impl Iterator<Item = u64> { std::iter::empty() }
        };
        let err = public_method(&method).unwrap_err();
        assert!(
            err.to_string()
                .contains("cannot use `impl Trait` as return type")
        );
    }

    #[test]
    fn test_new_constructor_valid() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub const fn new() -> Self {
                    Self { value: 0 }
                }
            }
        };
        let contract_struct: syn::ItemStruct = syn::parse_quote! {
            pub struct MyContract {
                value: u64,
            }
        };
        let impl_blocks = vec![&impl_block];
        assert!(new_constructor("MyContract", &impl_blocks, &contract_struct).is_ok());
    }

    #[test]
    fn test_new_constructor_valid_returns_typename() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub const fn new() -> MyContract {
                    MyContract { value: 0 }
                }
            }
        };
        let contract_struct: syn::ItemStruct = syn::parse_quote! {
            pub struct MyContract {
                value: u64,
            }
        };
        let impl_blocks = vec![&impl_block];
        assert!(new_constructor("MyContract", &impl_blocks, &contract_struct).is_ok());
    }

    #[test]
    fn test_new_constructor_missing() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn get_value(&self) -> u64 { 0 }
            }
        };
        let contract_struct: syn::ItemStruct = syn::parse_quote! {
            pub struct MyContract {
                value: u64,
            }
        };
        let impl_blocks = vec![&impl_block];
        let err = new_constructor("MyContract", &impl_blocks, &contract_struct).unwrap_err();
        assert!(err.to_string().contains("const fn new() -> Self"));
    }

    #[test]
    fn test_new_constructor_not_const() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn new() -> Self {
                    Self { value: 0 }
                }
            }
        };
        let contract_struct: syn::ItemStruct = syn::parse_quote! {
            pub struct MyContract {
                value: u64,
            }
        };
        let impl_blocks = vec![&impl_block];
        let err = new_constructor("MyContract", &impl_blocks, &contract_struct).unwrap_err();
        assert!(err.to_string().contains("must be a `const fn`"));
    }

    #[test]
    fn test_new_constructor_has_params() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub const fn new(value: u64) -> Self {
                    Self { value }
                }
            }
        };
        let contract_struct: syn::ItemStruct = syn::parse_quote! {
            pub struct MyContract {
                value: u64,
            }
        };
        let impl_blocks = vec![&impl_block];
        let err = new_constructor("MyContract", &impl_blocks, &contract_struct).unwrap_err();
        assert!(err.to_string().contains("must have no parameters"));
    }

    #[test]
    fn test_new_constructor_wrong_return() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub const fn new() -> u64 {
                    0
                }
            }
        };
        let contract_struct: syn::ItemStruct = syn::parse_quote! {
            pub struct MyContract {
                value: u64,
            }
        };
        let impl_blocks = vec![&impl_block];
        let err = new_constructor("MyContract", &impl_blocks, &contract_struct).unwrap_err();
        assert!(err.to_string().contains("must return `Self`"));
    }

    #[test]
    fn test_init_method_valid() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn init(&mut self, owner: Address) {
                    self.owner = owner;
                }
            }
        };
        let impl_blocks = vec![&impl_block];
        assert!(init_method("MyContract", &impl_blocks).is_ok());
    }

    #[test]
    fn test_init_method_valid_no_params() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn init(&mut self) {
                    self.initialized = true;
                }
            }
        };
        let impl_blocks = vec![&impl_block];
        assert!(init_method("MyContract", &impl_blocks).is_ok());
    }

    #[test]
    fn test_init_method_absent_is_ok() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn get_value(&self) -> u64 { 0 }
            }
        };
        let impl_blocks = vec![&impl_block];
        assert!(init_method("MyContract", &impl_blocks).is_ok());
    }

    #[test]
    fn test_init_method_immutable_self() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn init(&self, owner: Address) {}
            }
        };
        let impl_blocks = vec![&impl_block];
        let err = init_method("MyContract", &impl_blocks).unwrap_err();
        assert!(err.to_string().contains("must take `&mut self`"));
    }

    #[test]
    fn test_init_method_no_self() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn init(owner: Address) {}
            }
        };
        let impl_blocks = vec![&impl_block];
        let err = init_method("MyContract", &impl_blocks).unwrap_err();
        assert!(err.to_string().contains("must take `&mut self`"));
    }

    #[test]
    fn test_init_method_consuming_self() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn init(self, owner: Address) {}
            }
        };
        let impl_blocks = vec![&impl_block];
        let err = init_method("MyContract", &impl_blocks).unwrap_err();
        assert!(err.to_string().contains("must take `&mut self`"));
    }

    #[test]
    fn test_init_method_returns_value() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn init(&mut self, owner: Address) -> bool {
                    true
                }
            }
        };
        let impl_blocks = vec![&impl_block];
        let err = init_method("MyContract", &impl_blocks).unwrap_err();
        assert!(err.to_string().contains("must return `()`"));
    }

    #[test]
    fn test_init_method_returns_result() {
        let impl_block: ItemImpl = syn::parse_quote! {
            impl MyContract {
                pub fn init(&mut self, owner: Address) -> Result<(), Error> {
                    Ok(())
                }
            }
        };
        let impl_blocks = vec![&impl_block];
        let err = init_method("MyContract", &impl_blocks).unwrap_err();
        assert!(err.to_string().contains("must return `()`"));
    }

    #[test]
    fn test_trait_method_valid() {
        let method: ImplItemFn = syn::parse_quote! {
            fn owner(&self) -> Option<Address> { self.owner }
        };
        assert!(trait_method(&method, "OwnableTrait", false).is_ok());
    }

    #[test]
    fn test_trait_method_mut_self() {
        let method: ImplItemFn = syn::parse_quote! {
            fn transfer(&mut self, to: Address) {}
        };
        assert!(trait_method(&method, "OwnableTrait", false).is_ok());
    }

    #[test]
    fn test_trait_method_no_self_not_default() {
        // Non-default impl methods without self should fail
        let method: ImplItemFn = syn::parse_quote! {
            fn version() -> String { "1.0".to_string() }
        };
        let err = trait_method(&method, "ISemver", false).unwrap_err();
        assert!(err.to_string().contains("must have a `self` receiver"));
        assert!(err.to_string().contains("ISemver::version"));
    }

    #[test]
    fn test_trait_method_no_self_default_impl() {
        // Default impl methods (empty body) without self should pass
        let method: ImplItemFn = syn::parse_quote! {
            fn version() -> String {}
        };
        assert!(trait_method(&method, "ISemver", true).is_ok());
    }

    #[test]
    fn test_trait_method_consuming_self() {
        let method: ImplItemFn = syn::parse_quote! {
            fn destroy(self) {}
        };
        let err = trait_method(&method, "Destructible", false).unwrap_err();
        assert!(err.to_string().contains("cannot consume `self`"));
    }

    #[test]
    fn test_trait_method_generic() {
        let method: ImplItemFn = syn::parse_quote! {
            fn process<T>(&self, value: T) {}
        };
        let err = trait_method(&method, "Processor", false).unwrap_err();
        assert!(
            err.to_string()
                .contains("cannot have generic or const parameters")
        );
    }

    #[test]
    fn test_trait_method_async() {
        let method: ImplItemFn = syn::parse_quote! {
            async fn fetch(&self) -> Data {}
        };
        let err = trait_method(&method, "AsyncTrait", false).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("cannot be async"),
            "error should mention async: {msg}"
        );
        assert!(
            msg.contains("AsyncTrait::fetch"),
            "error should include trait::method name: {msg}"
        );
    }

    #[test]
    fn test_trait_method_impl_trait_param() {
        let method: ImplItemFn = syn::parse_quote! {
            fn process(&self, handler: impl Handler) {}
        };
        let err = trait_method(&method, "Processor", false).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("impl Trait"),
            "error should mention 'impl Trait': {msg}"
        );
        assert!(
            msg.contains("parameters"),
            "error should mention parameters: {msg}"
        );
    }

    #[test]
    fn test_trait_method_impl_trait_return() {
        let method: ImplItemFn = syn::parse_quote! {
            fn items(&self) -> impl Iterator<Item = u64> {}
        };
        let err = trait_method(&method, "Collection", false).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("impl Trait"),
            "error should mention 'impl Trait': {msg}"
        );
        assert!(
            msg.contains("return type"),
            "error should mention return type: {msg}"
        );
    }

    // ========================================================================
    // method_emits_event tests
    // ========================================================================

    #[test]
    fn test_method_emits_event_ref_self_no_emit_ok() {
        // &self methods don't need to emit events
        let method: ImplItemFn = syn::parse_quote! {
            pub fn get_value(&self) -> u64 { 0 }
        };
        assert!(method_emits_event(&method, false, false, false).is_ok());
    }

    #[test]
    fn test_method_emits_event_mut_self_with_emit_ok() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn set_value(&mut self, value: u64) { }
        };
        assert!(method_emits_event(&method, true, false, false).is_ok());
    }

    #[test]
    fn test_method_emits_event_mut_self_no_emit_error() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn set_value(&mut self, value: u64) { }
        };
        let err = method_emits_event(&method, false, false, false).unwrap_err();
        assert!(err.to_string().contains("emits no events"));
    }

    #[test]
    fn test_method_emits_event_mut_self_no_emit_suppressed() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn set_value(&mut self, value: u64) { }
        };
        assert!(method_emits_event(&method, false, true, false).is_ok());
    }

    #[test]
    fn test_method_emits_event_mut_self_with_manual_events() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn set_value(&mut self, value: u64) { }
        };
        assert!(method_emits_event(&method, false, false, true).is_ok());
    }

    #[test]
    fn test_method_emits_event_no_receiver_ok() {
        // Associated function (no self) doesn't need emit
        let method: ImplItemFn = syn::parse_quote! {
            pub fn default_value() -> u64 { 0 }
        };
        assert!(method_emits_event(&method, false, false, false).is_ok());
    }

    #[test]
    fn test_method_emits_event_consuming_self_ok() {
        // Consuming self (not &mut self) doesn't need emit check
        let method: ImplItemFn = syn::parse_quote! {
            pub fn into_value(self) -> u64 { 0 }
        };
        assert!(method_emits_event(&method, false, false, false).is_ok());
    }

    // =========================================================================
    // custom_handler tests
    // =========================================================================
    //
    // These exercise the validator the macro runs over each handler found in
    // the annotated module — the same code path a real `#[contract]`
    // expansion uses (see `extract::contract_data`). Signature shape is
    // independent per role, so every role gets a positive regression and a
    // tailored negative case.

    use crate::DataDriverRole;

    fn handler(role: DataDriverRole, func: syn::ItemFn) -> CustomDataDriverHandler {
        CustomDataDriverHandler {
            fn_name: "some_fn".to_string(),
            role,
            func,
        }
    }

    /// Canonical import map — mirrors what every real contract module
    /// carries for data-driver handlers. Shared across tests so short-path
    /// coverage matches the environment handlers actually compile in.
    fn canonical_imports() -> Vec<ImportInfo> {
        vec![
            ImportInfo {
                name: "Vec".into(),
                path: "alloc::vec::Vec".into(),
            },
            ImportInfo {
                name: "Error".into(),
                path: "dusk_data_driver::Error".into(),
            },
            ImportInfo {
                name: "JsonValue".into(),
                path: "dusk_data_driver::JsonValue".into(),
            },
        ]
    }

    #[test]
    fn test_custom_handler_encode_input_ok() {
        // Canonical encode_input handler — the shape the test-contract and
        // docs both use. Must keep passing or we've broken existing users.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_encoder(json: &str)
                -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::EncodeInput, func);
        assert!(custom_handler(&h, &[]).is_ok());
    }

    #[test]
    fn test_custom_handler_decode_input_ok() {
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_decoder(rkyv: &[u8])
                -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::DecodeInput, func);
        assert!(custom_handler(&h, &[]).is_ok());
    }

    #[test]
    fn test_custom_handler_decode_output_ok() {
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_decoder(bytes: &[u8])
                -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::DecodeOutput, func);
        assert!(custom_handler(&h, &[]).is_ok());
    }

    #[test]
    fn test_custom_handler_accepts_arbitrary_arg_name() {
        // Argument name is not part of the signature contract — the dispatch
        // site calls `handler(json)` / `handler(rkyv)` positionally, so any
        // name must work. Regression guard against the validator accidentally
        // pinning one specific name.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_encoder(anything: &str)
                -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::EncodeInput, func);
        assert!(custom_handler(&h, &[]).is_ok());
    }

    #[test]
    fn test_custom_handler_short_paths_resolve_through_imports() {
        // The import map is the single source of truth for path equivalence:
        // with canonical imports in scope the resolver rewrites short names
        // to canonical form, so short-path handlers match. (The end-to-end
        // trybuild fixture proves the *splicer* agrees — this covers the
        // validator's half of the contract.)
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_encoder(json: &str) -> Result<Vec<u8>, Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::EncodeInput, func);
        assert!(custom_handler(&h, &canonical_imports()).is_ok());
    }

    #[test]
    fn test_custom_handler_short_paths_rejected_without_imports() {
        // Absent the import map, `Vec<u8>` / `Error` resolve to themselves
        // and don't match the canonical. This is the opposite regression of
        // the positive test above: validates that resolution is what makes
        // short paths pass.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_encoder(json: &str) -> Result<Vec<u8>, Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::EncodeInput, func);
        assert!(custom_handler(&h, &[]).is_err());
    }

    #[test]
    fn test_custom_handler_accepts_handler_generic_lifetime() {
        // A handler that declares its own lifetime (`fn f<'a>(… &'a …)`) can
        // still bind whatever borrow the dispatcher passes. The resolver
        // strips reference lifetimes during canonicalisation, so the match
        // is automatic — this test pins that behaviour down so it doesn't
        // regress if the resolver ever starts preserving lifetimes.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_encoder<'a>(json: &'a str)
                -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::EncodeInput, func);
        assert!(custom_handler(&h, &[]).is_ok());
    }

    #[test]
    fn test_custom_handler_encode_input_wrong_arg_type() {
        // encode_input takes `&str`; this hands it `&[u8]` (the decoder
        // shape). The validator must surface this before the downstream
        // type error fires against macro-generated code.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_encoder(json: &[u8])
                -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::EncodeInput, func);
        let err = custom_handler(&h, &[]).unwrap_err().to_string();
        assert!(
            err.contains("my_encoder"),
            "error should name the handler: {err}"
        );
        assert!(
            err.contains("encode_input"),
            "error should name the role: {err}"
        );
        assert!(
            err.contains("fn(&str) -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error>"),
            "error should show the full expected signature: {err}"
        );
    }

    #[test]
    fn test_custom_handler_decode_input_wrong_return_type() {
        // decode_input must return `Result<JsonValue, Error>`; this returns
        // a raw `Vec<u8>`, which is the wrong role's return shape.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_decoder(rkyv: &[u8]) -> alloc::vec::Vec<u8>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::DecodeInput, func);
        let err = custom_handler(&h, &[]).unwrap_err().to_string();
        assert!(err.contains("my_decoder"), "names the handler: {err}");
        assert!(err.contains("decode_input"), "names the role: {err}");
        assert!(
            err.contains(
                "fn(&[u8]) -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error>"
            ),
            "shows the expected signature: {err}"
        );
    }

    #[test]
    fn test_custom_handler_decode_output_wrong_arg_count() {
        // decode_output takes exactly one argument; more than one signals a
        // misunderstanding of the dispatch contract.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_decoder(a: &[u8], b: &[u8])
                -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::DecodeOutput, func);
        let err = custom_handler(&h, &[]).unwrap_err().to_string();
        assert!(err.contains("my_decoder"), "names the handler: {err}");
        assert!(err.contains("decode_output"), "names the role: {err}");
        assert!(
            err.contains("exactly one argument"),
            "error should explain the argument-count requirement: {err}"
        );
        assert!(
            err.contains(
                "fn(&[u8]) -> Result<dusk_data_driver::JsonValue, dusk_data_driver::Error>"
            ),
            "shows the expected signature: {err}"
        );
    }

    #[test]
    fn test_custom_handler_no_return_type() {
        // A handler with no return at all can't participate in dispatch —
        // catch it early with a role-specific message instead of a downstream
        // `()` vs `Result<...>` mismatch.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_encoder(json: &str) { }
        };
        let h = handler(DataDriverRole::EncodeInput, func);
        let err = custom_handler(&h, &[]).unwrap_err().to_string();
        assert!(err.contains("my_encoder"), "names the handler: {err}");
        assert!(err.contains("encode_input"), "names the role: {err}");
        assert!(
            err.contains("must return a `Result`"),
            "error should explain the return requirement: {err}"
        );
    }

    #[test]
    fn test_custom_handler_no_args() {
        // Zero arguments also falls under the "exactly one" rule — the
        // validator should say so explicitly.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_encoder()
                -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::EncodeInput, func);
        let err = custom_handler(&h, &[]).unwrap_err().to_string();
        assert!(
            err.contains("exactly one argument"),
            "error should explain the argument-count requirement: {err}"
        );
    }

    #[test]
    fn test_custom_handler_rejects_static_lifetime_on_argument() {
        // The generated dispatcher passes a local-lifetime borrow; a handler
        // that promises `'static` can't bind it. Catch it at the validator
        // with a lifetime-specific message rather than letting a lifetime
        // mismatch surface deep in macro-generated code.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_encoder(json: &'static str)
                -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::EncodeInput, func);
        let err = custom_handler(&h, &[]).unwrap_err().to_string();
        assert!(
            err.contains("'static"),
            "error should name the offending lifetime: {err}"
        );
        assert!(
            err.contains("my_encoder"),
            "error should name the handler: {err}"
        );
        assert!(
            err.contains("encode_input"),
            "error should name the role: {err}"
        );
    }

    #[test]
    fn test_custom_handler_rejects_static_lifetime_in_return_position() {
        // `'static` anywhere in the return type is equally unworkable — the
        // dispatcher can't supply a `'static` reference back either. Same
        // reject path, exercised on the return side for symmetry.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_decoder(rkyv: &[u8])
                -> Result<&'static dusk_data_driver::JsonValue, dusk_data_driver::Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::DecodeInput, func);
        let err = custom_handler(&h, &[]).unwrap_err().to_string();
        assert!(
            err.contains("'static"),
            "error should name the offending lifetime: {err}"
        );
    }

    #[test]
    fn test_custom_handler_rejects_different_error_prefix() {
        // `foo::Error` shares the last segment with `dusk_data_driver::Error`
        // but is a different type. No import map entry rewrites it, so the
        // canonical-form comparison fails and the validator rejects.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_encoder(json: &str)
                -> Result<alloc::vec::Vec<u8>, foo::Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::EncodeInput, func);
        let err = custom_handler(&h, &[]).unwrap_err().to_string();
        assert!(
            err.contains("has return type"),
            "error should identify the return type as the mismatch: {err}"
        );
        assert!(
            err.contains("foo::Error"),
            "error should surface what the user wrote: {err}"
        );
    }

    #[test]
    fn test_custom_handler_rejects_different_error_name() {
        // `MyError` isn't in the import map, resolves to itself, and differs
        // from canonical `dusk_data_driver::Error`. The rejected type must
        // surface in the message so the user can find the mismatch.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_encoder(json: &str)
                -> Result<alloc::vec::Vec<u8>, MyError>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::EncodeInput, func);
        let err = custom_handler(&h, &[]).unwrap_err().to_string();
        assert!(
            err.contains("MyError"),
            "error should surface the user's wrong type: {err}"
        );
    }

    #[test]
    fn test_custom_handler_rejects_mut_reference() {
        // `&mut str` has the wrong mutability — still a reference, still to
        // `str`, but semantically different and would break the generated
        // call site. Canonicalisation preserves mutability, so the compare
        // catches this.
        let func: syn::ItemFn = syn::parse_quote! {
            fn my_encoder(json: &mut str)
                -> Result<alloc::vec::Vec<u8>, dusk_data_driver::Error>
            { unimplemented!() }
        };
        let h = handler(DataDriverRole::EncodeInput, func);
        let err = custom_handler(&h, &[]).unwrap_err().to_string();
        assert!(
            err.contains("has argument type"),
            "error should identify the argument type as the mismatch: {err}"
        );
    }

    // =========================================================================
    // tokens_equal tests
    // =========================================================================
    //
    // `quote!`-produced strings carry whitespace `rustc`'s type printer
    // doesn't (`& str`, `Result < T , E >`), while `resolve::resolve_type`
    // emits whitespace-free output. The comparator has to bridge the two
    // without re-parsing — unit-test it directly so a regression surfaces
    // here, not as a confusing "argument type doesn't match" from a handler
    // that looked fine.

    #[test]
    fn test_tokens_equal_ignores_whitespace_differences() {
        // `quote!(&str).to_string()` gives `& str`; the resolver emits
        // `&str`. Both must compare equal or short-path canonicalisation
        // never matches canonical.
        assert!(tokens_equal("& str", "&str"));
        assert!(tokens_equal(
            "Result < alloc :: vec :: Vec < u8 > , dusk_data_driver :: Error >",
            "Result<alloc::vec::Vec<u8>, dusk_data_driver::Error>",
        ));
        assert!(tokens_equal("  foo  ::  bar  ", "foo::bar"));
        assert!(tokens_equal("&[u8]", "& [u8]"));
    }

    #[test]
    fn test_tokens_equal_detects_real_differences() {
        // Mutability, identifier, and path differences must not collapse
        // under whitespace stripping.
        assert!(!tokens_equal("&str", "&mut str"));
        assert!(!tokens_equal("dusk_data_driver::Error", "foo::Error"));
        assert!(!tokens_equal("Vec<u8>", "Vec<u16>"));
        assert!(!tokens_equal("Error", "MyError"));
    }
}
