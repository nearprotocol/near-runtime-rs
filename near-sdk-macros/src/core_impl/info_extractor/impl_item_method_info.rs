use crate::core_impl::info_extractor::AttrSigInfo;
use crate::core_impl::utils;
use quote::ToTokens;
use syn::{ImplItemFn as ImplItemMethod, Type, Visibility};

/// Information extracted from `ImplItemMethod`.
pub struct ImplItemMethodInfo {
    /// Information on the attributes and the signature of the method.
    pub attr_signature_info: AttrSigInfo,
    /// The type of the contract struct.
    pub struct_type: Type,
}

impl ImplItemMethodInfo {
    /// Process the method and extract information important for near-sdk.
    pub fn new(
        original: &mut ImplItemMethod,
        is_trait_impl: bool,
        struct_type: Type,
    ) -> syn::Result<Option<Self>> {
        let ImplItemMethod { attrs, sig, .. } = original;
        utils::sig_is_supported(sig)?;
        if is_trait_impl || matches!(original.vis, Visibility::Public(_)) {
            let source_type = &struct_type.to_token_stream();
            let attr_signature_info = AttrSigInfo::new(attrs, sig, source_type)?;
            Ok(Some(Self { attr_signature_info, struct_type }))
        } else {
            Ok(None)
        }
    }
}

// Rustfmt removes comas.
#[rustfmt::skip]
#[cfg(test)]
mod tests {
    use syn::{parse_quote, Type, ImplItemFn as ImplItemMethod , ReturnType};
    use crate::core_impl::{ImplItemMethodInfo};

    #[test]
    fn init_no_return() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemMethod = parse_quote! {
            #[init]
            pub fn method(k: &mut u64) { }
        };
        let actual = ImplItemMethodInfo::new(&mut method, false, impl_type).map(|_| ()).unwrap_err();
        let expected = "Init function must return the contract state.";
        assert_eq!(expected, actual.to_string());
    }

    #[test]
    fn init_result_return() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemMethod = parse_quote! {
            #[init]
            #[handle_result]
            pub fn method(k: &mut u64) -> Result<Self, Error> { }
        };
        let method = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method.attr_signature_info.returns.original;
        let expected: Type = syn::parse_str("Result<Hello, Error>").unwrap();
        assert!(matches!(actual, ReturnType::Type(_, ty) if ty.as_ref() == &expected));
    }

    #[test]
    fn handle_result_incorrect_return_type() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemMethod = parse_quote! {
            #[handle_result]
            pub fn method(&self) -> &'static str { }
        };
        let actual = ImplItemMethodInfo::new(&mut method, false, impl_type).map(|_| ()).unwrap_err();
        let expected = "Function marked with #[handle_result] should return Result<T, E> (where E implements FunctionError). If you're trying to use a type alias for `Result`, try `#[handle_result(aliased)]`.";
        assert_eq!(expected, actual.to_string());
    }

    #[test]
    fn payable_self_by_value_fails() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemMethod = parse_quote! {
            #[payable]
            pub fn method(self) -> Self { }
        };
        let actual = ImplItemMethodInfo::new(&mut method, false, impl_type).map(|_| ()).unwrap_err();
        let expected = "View function can't be payable.";
        assert_eq!(expected.to_string(), actual.to_string());
    }
}
