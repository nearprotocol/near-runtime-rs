use crate::core_impl::ext::generate_ext_function_wrappers;
use crate::core_impl::utils;
use crate::core_impl::ReturnKind;
use crate::ItemImplInfo;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, ToTokens};
use syn::{spanned::Spanned, Ident};

impl ItemImplInfo {
    /// Generate the code that wraps
    pub fn wrapper_code(&self) -> TokenStream2 {
        let mut res = TokenStream2::new();
        let mut checks = quote! {};
        for method in &self.methods {
            res.extend(method.method_wrapper());
            if let ReturnKind::HandlesResultImplicit { .. } =
                method.attr_signature_info.returns.kind
            {
                let error_type = match &method.attr_signature_info.returns.original {
                    syn::ReturnType::Default => quote! { () },
                    syn::ReturnType::Type(_, ty) => {
                        let error_type = utils::extract_error_type(ty);
                        quote! { #error_type }
                    }
                };
                let method_name = &method.attr_signature_info.ident;
                let check_trait_method_name =
                    format_ident!("assert_implements_my_trait{}", method_name);

                checks.extend(quote! {
                    fn #check_trait_method_name() {
                        let _ = near_sdk::check_contract_error_trait as fn(&#error_type);
                    }
                });
            }
        }
        let current_type = &self.ty;
        res.extend(quote! {
            impl #current_type {
                #checks
            }
        });
        res
    }

    pub fn generate_ext_wrapper_code(&self) -> TokenStream2 {
        match syn::parse::<Ident>(self.ty.to_token_stream().into()) {
            Ok(n) => generate_ext_function_wrappers(
                &n,
                self.methods.iter().map(|m| &m.attr_signature_info),
            ),
            Err(e) => syn::Error::new(self.ty.span(), e).to_compile_error(),
        }
    }

    pub fn generate_error_methods(&self) -> TokenStream2 {
        let mut error_methods = quote! {};

        self.methods.iter().map(|m| &m.attr_signature_info).for_each(|method| {
            let error_method_name = quote::format_ident!("{}_error", method.ident);

            if let ReturnKind::HandlesResultImplicit(status) = &method.returns.kind {
                if status.persist_on_error {
                    let error_type = crate::get_error_type_from_status(status);
                    let panic_tokens = crate::standardized_error_panic_tokens();

                    let ty = self.ty.to_token_stream();

                    error_methods.extend(quote! {
                        #[near]
                        impl #ty {
                            pub fn #error_method_name(&self, err: #error_type) {
                                #panic_tokens
                            }
                        }
                    });
                }
            }
        });

        error_methods
    }
}
// Rustfmt removes comas.
#[rustfmt::skip]
#[cfg(test)]
mod tests {
    use syn::{parse_quote, ImplItemFn, Type};
    use crate::core_impl::info_extractor::{ImplItemMethodInfo, ItemImplInfo};
    use crate::core_impl::utils::test_helpers::{local_insta_assert_snapshot, pretty_print_syn_str};


    #[test]
    fn trait_implt() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = syn::parse_str("fn method(&self) { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, true, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn no_args_no_return_no_mut() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = syn::parse_str("pub fn method(&self) { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn owned_no_args_no_return_no_mut() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = syn::parse_str("pub fn method(self) { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }


    #[test]
    fn mut_owned_no_args_no_return() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = syn::parse_str("pub fn method(mut self) { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn no_args_no_return_mut() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = syn::parse_str("pub fn method(&mut self) { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn arg_no_return_no_mut() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = syn::parse_str("pub fn method(&self, k: u64) { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn args_no_return_mut() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn =
            syn::parse_str("pub fn method(&mut self, k: u64, m: Bar) { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn args_return_mut() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn =
            syn::parse_str("pub fn method(&mut self, k: u64, m: Bar) -> Option<u64> { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn args_return_ref() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn =
            syn::parse_str("pub fn method(&self) -> &Option<u64> { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn arg_ref() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = syn::parse_str("pub fn method(&self, k: &u64) { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn arg_mut_ref() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn =
            syn::parse_str("pub fn method(&self, k: &mut u64) { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn callback_args() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[private] pub fn method(&self, #[callback_unwrap] x: &mut u64, y: ::std::string::String, #[callback_unwrap] z: ::std::vec::Vec<u8>) { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn callback_args_only() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[private] pub fn method(&self, #[callback_unwrap] x: &mut u64, #[callback_unwrap] y: ::std::string::String) { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn callback_args_results() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[private] pub fn method(&self, #[callback_result] x: &mut Result<u64, PromiseError>, #[callback_result] y: Result<::std::string::String, PromiseError>) { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn callback_args_vec() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[private] pub fn method(&self, #[callback_vec] x: Vec<String>, y: String) { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn simple_init() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[init]
            pub fn method(k: &mut u64) -> Self { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn init_no_return() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[init]
            pub fn method(k: &mut u64) { }
        };
        let actual = ImplItemMethodInfo::new(&mut method, false, impl_type).map(|_| ()).unwrap_err();
        let expected = "Init function must return the contract state.";
        assert_eq!(expected, actual.to_string());
    }

    #[test]
    fn init_ignore_state() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[init(ignore_state)]
            pub fn method(k: &mut u64) -> Self { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn init_payable() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[init]
            #[payable]
            pub fn method(k: &mut u64) -> Self { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn args_return_mut_borsh() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[result_serializer(borsh)]
            pub fn method(&mut self, #[serializer(borsh)] k: u64, #[serializer(borsh)]m: Bar) -> Option<u64> { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn callback_args_mixed_serialization() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[private] pub fn method(&self, #[callback_unwrap] #[serializer(borsh)] x: &mut u64, #[serializer(borsh)] y: ::std::string::String, #[callback_unwrap] #[serializer(json)] z: ::std::vec::Vec<u8>) { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn no_args_no_return_mut_payable() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = syn::parse_str("#[payable] pub fn method(&mut self) { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn private_method() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = syn::parse_str("#[private] pub fn private_method(&mut self) { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn handle_result_json() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[handle_result]
            pub fn method(&self) -> Result::<u64, &'static str> { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }
    
    #[test]
    fn handle_result_mut() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[handle_result]
            pub fn method(&mut self) -> Result<u64, &'static str> { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn handle_result_borsh() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[handle_result]
            #[result_serializer(borsh)]
            pub fn method(&self) -> Result<u64, &'static str> { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn handle_result_init() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[init]
            #[handle_result]
            pub fn new() -> Result<Self, &'static str> { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn handle_result_init_ignore_state() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[init(ignore_state)]
            #[handle_result]
            pub fn new() -> Result<Self, &'static str> { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn handle_no_self() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = syn::parse_str("pub fn method() { }").unwrap();
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn result_implicit() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            pub fn method(&self) -> Result<u64, &'static str> { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn persist_on_error() {
        let impl_type: Type = syn::parse_str("Hello").unwrap();
        let mut method: ImplItemFn = parse_quote! {
            #[persist_on_error]
            pub fn method(&mut self) -> Result<u64, &'static str> { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.method_wrapper();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }

    #[test]
    fn generated_method_error() {
        let mut impl_contract: syn::ItemImpl = parse_quote! {
            impl Contract {
                #[persist_on_error]
                pub fn method(&mut self) -> Result<u64, &'static str> { }
            }
        };
        let impl_contract_info = ItemImplInfo::new(&mut impl_contract).unwrap();
        let actual = impl_contract_info.generate_error_methods();
        local_insta_assert_snapshot!(pretty_print_syn_str(&actual).unwrap());
    }
}
