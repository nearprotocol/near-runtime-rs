use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{parse_quote, Attribute, Expr, Lit::Str, Meta::NameValue, MetaNameValue, Type};

use crate::core_impl::{
    utils, BindgenArgType, ImplItemMethodInfo, ItemImplInfo, MethodKind, ReturnKind, SerializerType,
};

pub fn generate(i: &ItemImplInfo) -> TokenStream2 {
    if i.methods.is_empty() {
        // Short-circuit if there are no public functions to export to ABI
        return TokenStream2::new();
    }

    let functions: Vec<TokenStream2> = i.methods.iter().map(|m| m.abi_struct()).collect();
    let first_function_name = &i.methods[0].attr_signature_info.ident;
    let near_abi_symbol = format_ident!("__near_abi_{}", first_function_name);
    quote! {
        #[cfg(not(target_arch = "wasm32"))]
        const _: () = {
            #[no_mangle]
            pub extern "C" fn #near_abi_symbol() -> (*const u8, usize) {
                use ::std::string::String;

                let mut gen = ::near_sdk::schemars::gen::SchemaGenerator::default();
                let functions = vec![#(#functions),*];
                let mut data = ::std::mem::ManuallyDrop::new(
                    ::near_sdk::serde_json::to_vec(&::near_sdk::__private::ChunkedAbiEntry::new(
                        functions,
                        gen.into_root_schema_for::<String>(),
                    ))
                    .unwrap(),
                );
                data.shrink_to_fit();
                assert!(data.len() == data.capacity());
                (data.as_ptr(), data.len())
            }
        };
    }
}

impl ImplItemMethodInfo {
    /// Generates ABI struct for this function.
    ///
    /// # Example:
    /// The following function:
    /// ```ignore
    /// /// I am a function.
    /// #[handle_result]
    /// pub fn f3(&mut self, arg0: FancyStruct, arg1: u64) -> Result<IsOk, Error> { }
    /// ```
    /// will produce this struct:
    /// ```ignore
    /// near_sdk::__private::AbiFunction {
    ///     name: "f3".to_string(),
    ///     doc: Some(" I am a function.".to_string()),
    ///     kind: near_sdk::__private::AbiFunctionKind::Call,
    ///     modifiers: vec![],
    ///     params: near_sdk::__private::AbiParameters::Json {
    ///         args: vec![
    ///             near_sdk::__private::AbiJsonParameter {
    ///                 name: "arg0".to_string(),
    ///                 type_schema: gen.subschema_for::<FancyStruct>(),
    ///             },
    ///             near_sdk::__private::AbiJsonParameter {
    ///                 name: "arg1".to_string(),
    ///                 type_schema: gen.subschema_for::<u64>(),
    ///             }
    ///         ]
    ///     },
    ///     callbacks: vec![],
    ///     callbacks_vec: None,
    ///     result: Some(near_sdk::__private::AbiType::Json {
    ///         type_schema: gen.subschema_for::<IsOk>(),
    ///     })
    /// }
    /// ```
    /// If args are serialized with Borsh it will not include `#[derive(::near_sdk::borsh::BorshSchema)]`.
    pub fn abi_struct(&self) -> TokenStream2 {
        let attr_signature_info = &self.attr_signature_info;

        let function_name_str = attr_signature_info.ident.to_string();
        let function_doc = match parse_rustdoc(&attr_signature_info.non_bindgen_attrs) {
            Some(doc) => quote! { ::std::option::Option::Some(::std::string::String::from(#doc)) },
            None => quote! { ::std::option::Option::None },
        };
        let mut modifiers = vec![];
        let kind = match &attr_signature_info.method_kind {
            MethodKind::View(_) => quote! { ::near_sdk::__private::AbiFunctionKind::View },
            MethodKind::Call(_) => {
                quote! { ::near_sdk::__private::AbiFunctionKind::Call }
            }
            MethodKind::Init(_) => {
                modifiers.push(quote! { ::near_sdk::__private::AbiFunctionModifier::Init });
                quote! { ::near_sdk::__private::AbiFunctionKind::Call }
            }
        };
        if attr_signature_info.is_payable() {
            modifiers.push(quote! { ::near_sdk::__private::AbiFunctionModifier::Payable });
        }
        if attr_signature_info.is_private() {
            modifiers.push(quote! { ::near_sdk::__private::AbiFunctionModifier::Private });
        }
        let modifiers = quote! {
            ::std::vec![#(#modifiers),*]
        };

        let mut params = Vec::<TokenStream2>::new();
        let mut callbacks = Vec::<TokenStream2>::new();
        let mut callback_vec: Option<TokenStream2> = None;
        for arg in &attr_signature_info.args {
            let typ = &arg.ty;
            let arg_name = arg.ident.to_string();
            match arg.bindgen_ty {
                BindgenArgType::Regular => {
                    let schema = generate_schema(typ, &arg.serializer_ty);
                    match arg.serializer_ty {
                        SerializerType::JSON => params.push(quote! {
                            ::near_sdk::__private::AbiJsonParameter {
                                name: ::std::string::String::from(#arg_name),
                                type_schema: #schema,
                            }
                        }),
                        SerializerType::Borsh => params.push(quote! {
                            ::near_sdk::__private::AbiBorshParameter {
                                name: ::std::string::String::from(#arg_name),
                                type_schema: #schema,
                            }
                        }),
                    };
                }
                BindgenArgType::CallbackArg => {
                    callbacks.push(generate_abi_type(typ, &arg.serializer_ty));
                }
                BindgenArgType::CallbackResultArg => {
                    let typ = if let Some(ok_type) = utils::extract_ok_type(typ) {
                        ok_type
                    } else {
                        return syn::Error::new_spanned(
                            &arg.ty,
                            "Function parameters marked with \
                                #[callback_result] should have type Result<T, PromiseError>",
                        )
                        .into_compile_error();
                    };
                    callbacks.push(generate_abi_type(typ, &arg.serializer_ty));
                }
                BindgenArgType::CallbackArgVec => {
                    if callback_vec.is_none() {
                        let typ = if let Some(vec_type) = utils::extract_vec_type(typ) {
                            vec_type
                        } else {
                            return syn::Error::new_spanned(
                                &arg.ty,
                                "Function parameters marked with #[callback_vec] should have type Vec<T>",
                            )
                            .into_compile_error();
                        };

                        callback_vec = Some(self.abi_callback_vec_tokens(typ));
                    } else {
                        return syn::Error::new(
                            Span::call_site(),
                            "A function can only have one #[callback_vec] parameter.",
                        )
                        .to_compile_error();
                    }
                }
            };
        }
        let params = match attr_signature_info.input_serializer {
            SerializerType::JSON => quote! {
                ::near_sdk::__private::AbiParameters::Json {
                    args: ::std::vec![#(#params),*]
                }
            },
            SerializerType::Borsh => quote! {
                ::near_sdk::__private::AbiParameters::Borsh {
                    args: ::std::vec![#(#params),*]
                }
            },
        };
        let callback_vec = callback_vec.unwrap_or(quote! { ::std::option::Option::None });

        let result = self.abi_result_tokens();

        quote! {
             ::near_sdk::__private::AbiFunction {
                 name: ::std::string::String::from(#function_name_str),
                 doc: #function_doc,
                 kind: #kind,
                 modifiers: #modifiers,
                 params: #params,
                 callbacks: ::std::vec![#(#callbacks),*],
                 callbacks_vec: #callback_vec,
                 result: #result
             }
        }
    }

    fn abi_result_tokens(&self) -> TokenStream2 {
        use ReturnKind::*;

        match &self.attr_signature_info.returns.kind {
            Default => quote! { ::std::option::Option::None },
            General(ty) => self.abi_result_tokens_with_return_value(ty),
            HandlesResultExplicit(ty) => {
                // extract the `Ok` type from the result
                let ty = parse_quote! { <#ty as near_sdk::__private::ResultTypeExt>::Okay };
                self.abi_result_tokens_with_return_value(&ty)
            }
            HandlesResultImplicit(status_result) => {
                let ty = &status_result.result_type;
                let ty = parse_quote! { <#ty as near_sdk::__private::ResultTypeExt>::Okay };
                self.abi_result_tokens_with_return_value(&ty)
            }
        }
    }

    fn abi_result_tokens_with_return_value(&self, return_value_type: &Type) -> TokenStream2 {
        use MethodKind::*;

        let some_abi_type = |result_serializer: &SerializerType| {
            let abi_type = generate_abi_type(return_value_type, result_serializer);
            quote! { ::std::option::Option::Some(#abi_type) }
        };

        match &self.attr_signature_info.method_kind {
            Call(call_method) => some_abi_type(&call_method.result_serializer),
            // Init methods don't return a value, they just save the newly created contract state.
            Init(_) => quote! { ::std::option::Option::None },
            View(view_method) => some_abi_type(&view_method.result_serializer),
        }
    }

    fn abi_callback_vec_tokens(&self, callback_vec_type: &Type) -> TokenStream2 {
        let abi_type = |result_serializer: &SerializerType| {
            let tokens = generate_abi_type(callback_vec_type, result_serializer);
            quote! {
                ::std::option::Option::Some(#tokens)
            }
        };

        match &self.attr_signature_info.method_kind {
            MethodKind::Call(call_method) => abi_type(&call_method.result_serializer),
            MethodKind::Init(_) => quote! { ::std::option::Option::None },
            MethodKind::View(view_method) => abi_type(&view_method.result_serializer),
        }
    }
}

fn generate_schema(ty: &Type, serializer_type: &SerializerType) -> TokenStream2 {
    match serializer_type {
        SerializerType::JSON => quote! {
            gen.subschema_for::<#ty>()
        },
        SerializerType::Borsh => quote! {
            ::near_sdk::borsh::schema_container_of::<#ty>()
        },
    }
}

fn generate_abi_type(ty: &Type, serializer_type: &SerializerType) -> TokenStream2 {
    let schema = generate_schema(ty, serializer_type);
    match serializer_type {
        SerializerType::JSON => quote! {
            ::near_sdk::__private::AbiType::Json {
                type_schema: #schema,
            }
        },
        SerializerType::Borsh => quote! {
            ::near_sdk::__private::AbiType::Borsh {
                type_schema: #schema,
            }
        },
    }
}

pub fn parse_rustdoc(attrs: &[Attribute]) -> Option<String> {
    let doc = attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc") {
                if let NameValue(MetaNameValue { value: Expr::Lit(value), .. }) = attr.meta.clone()
                {
                    if let Str(doc) = value.lit {
                        return Some(doc.value());
                    }
                }
            }
            None
        })
        .collect::<Vec<_>>()
        .join("\n");

    if doc.is_empty() {
        None
    } else {
        Some(doc)
    }
}

// Rustfmt removes comas.
#[rustfmt::skip]
#[cfg(test)]
mod tests {
    use proc_macro2::TokenStream;
    use syn::{parse_quote, Type};
    use crate::core_impl::ImplItemMethodInfo;
    use crate::core_impl::utils::test_helpers::{local_insta_assert_snapshot, pretty_print_syn_str};
    use quote::quote;


    fn pretty_print_fn_body_syn_str(input: TokenStream) -> String {
        let input =  quote!(
            fn main() {
            #input
            }
        );
        let res = pretty_print_syn_str(&input).unwrap();
       res.strip_prefix("fn main() {\n").unwrap().strip_suffix("}\n").unwrap().to_string()
    }
    
    #[test]
    fn test_generate_abi_fallible_json() {
        let impl_type: Type = syn::parse_str("Test").unwrap();
        let mut method = parse_quote! {
            /// I am a function.
            #[handle_result]
            pub fn f3(&mut self, arg0: FancyStruct, arg1: u64) -> Result<IsOk, Error> { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.abi_struct();

        local_insta_assert_snapshot!(pretty_print_fn_body_syn_str(actual));
    }

    #[test]
    fn test_generate_abi_fallible_borsh() {
        let impl_type: Type = syn::parse_str("Test").unwrap();
        let mut method = parse_quote! {
            #[result_serializer(borsh)]
            #[payable]
            #[handle_result]
            pub fn f3(&mut self, #[serializer(borsh)] arg0: FancyStruct) -> Result<IsOk, Error> { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.abi_struct();

        local_insta_assert_snapshot!(pretty_print_fn_body_syn_str(actual));
    }
    
    #[test]
    fn test_generate_abi_private_callback_vec() {
        let impl_type: Type = syn::parse_str("Test").unwrap();
        let mut method = parse_quote! {
            #[private] 
            pub fn method(
                &self, 
                #[callback_vec] x: Vec<String>, 
            ) -> bool { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.abi_struct();
       
        local_insta_assert_snapshot!(pretty_print_fn_body_syn_str(actual));
    }
    
    #[test]
    fn test_generate_abi_callback_args() {
        let impl_type: Type = syn::parse_str("Test").unwrap();
        let mut method = parse_quote! {
            pub fn method(&self, #[callback_unwrap] #[serializer(borsh)] x: &mut u64, #[serializer(borsh)] y: String, #[callback_unwrap] #[serializer(json)] z: Vec<u8>) { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.abi_struct();

        local_insta_assert_snapshot!(pretty_print_fn_body_syn_str(actual));
    }
    
    #[test]
    fn test_generate_abi_init_ignore_state() {
        let impl_type: Type = syn::parse_str("Test").unwrap();
        let mut method = parse_quote! {
            #[init(ignore_state)]
            pub fn new() -> u64 { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.abi_struct();

        local_insta_assert_snapshot!(pretty_print_fn_body_syn_str(actual));
    }
    
    #[test]
    fn test_generate_abi_no_return() {
        let impl_type: Type = syn::parse_str("Test").unwrap();
        let mut method = parse_quote! {
            pub fn method() { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.abi_struct();

        local_insta_assert_snapshot!(pretty_print_fn_body_syn_str(actual));
    }
}
