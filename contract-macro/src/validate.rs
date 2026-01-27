// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Validation functions for contract macro.

use syn::{FnArg, ImplItem, ImplItemFn, ItemImpl, ReturnType, Type, Visibility};

/// Validate that a public method has a supported signature for extern wrapper generation.
///
/// Returns an error if the method:
/// - Has no `self` receiver (associated function)
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

    // Check for self receiver
    let receiver = method.sig.inputs.first().and_then(|arg| {
        if let FnArg::Receiver(r) = arg {
            Some(r)
        } else {
            None
        }
    });

    let Some(receiver) = receiver else {
        return Err(syn::Error::new_spanned(
            &method.sig,
            format!(
                "public method `{name}` must have a `self` receiver; \
                 associated functions cannot be exposed as contract methods"
            ),
        ));
    };

    // Check that self is borrowed, not consumed
    if receiver.reference.is_none() {
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
/// For default implementations (empty body), associated functions (no self) are allowed.
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
            pub fn new() -> Self { Self }
        };
        let err = public_method(&method).unwrap_err();
        assert!(err.to_string().contains("must have a `self` receiver"));
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
        assert!(err
            .to_string()
            .contains("cannot have generic or const parameters"));
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
        assert!(err
            .to_string()
            .contains("cannot have generic or const parameters"));
    }

    #[test]
    fn test_validate_method_impl_trait_param() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn process(&self, x: impl Display) {}
        };
        let err = public_method(&method).unwrap_err();
        assert!(err
            .to_string()
            .contains("cannot use `impl Trait` in parameters"));
    }

    #[test]
    fn test_validate_method_impl_trait_return() {
        let method: ImplItemFn = syn::parse_quote! {
            pub fn iter(&self) -> impl Iterator<Item = u64> { std::iter::empty() }
        };
        let err = public_method(&method).unwrap_err();
        assert!(err
            .to_string()
            .contains("cannot use `impl Trait` as return type"));
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
        assert!(err
            .to_string()
            .contains("cannot have generic or const parameters"));
    }

    #[test]
    fn test_trait_method_async() {
        let method: ImplItemFn = syn::parse_quote! {
            async fn fetch(&self) -> Data {}
        };
        let err = trait_method(&method, "AsyncTrait", false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("cannot be async"), "error should mention async: {msg}");
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
}
