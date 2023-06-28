use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{Attribute, Lit::Str, Meta::NameValue, MetaNameValue, ReturnType, Type};

use crate::core_impl::{
    utils, AttrSigInfoV1, BindgenArgType, ImplItemMethodInfo, ItemImplInfo, MethodType,
    SerializerType,
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
                let mut gen = near_sdk::__private::schemars::gen::SchemaGenerator::default();
                let functions = vec![#(#functions),*];
                let mut data = std::mem::ManuallyDrop::new(
                    near_sdk::serde_json::to_vec(&near_sdk::__private::ChunkedAbiEntry::new(
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
    ///     type_schema: gen.subschema_for::<IsOk>(),
    ///     })
    /// }
    /// ```
    /// If args are serialized with Borsh it will not include `#[derive(borsh::BorshSchema)]`.
    pub fn abi_struct(&self) -> TokenStream2 {
        // FIXME: Refactor to use `AttrSigInfoV2`
        // Tracking issue: https://github.com/near/near-sdk-rs/issues/1032
        let attr_signature_info: AttrSigInfoV1 = self.attr_signature_info.clone().into();

        let function_name_str = attr_signature_info.ident.to_string();
        let function_doc = match parse_rustdoc(&attr_signature_info.non_bindgen_attrs) {
            Some(doc) => quote! { Some(#doc.to_string()) },
            None => quote! { None },
        };
        let mut modifiers = vec![];
        let kind = match &attr_signature_info.method_type {
            &MethodType::View => quote! { near_sdk::__private::AbiFunctionKind::View },
            &MethodType::Regular => {
                quote! { near_sdk::__private::AbiFunctionKind::Call }
            }
            &MethodType::Init | &MethodType::InitIgnoreState => {
                modifiers.push(quote! { near_sdk::__private::AbiFunctionModifier::Init });
                quote! { near_sdk::__private::AbiFunctionKind::Call }
            }
        };
        if attr_signature_info.is_payable {
            modifiers.push(quote! { near_sdk::__private::AbiFunctionModifier::Payable });
        }
        if attr_signature_info.is_private {
            modifiers.push(quote! { near_sdk::__private::AbiFunctionModifier::Private });
        }
        let modifiers = quote! {
            vec![#(#modifiers),*]
        };
        let AttrSigInfoV1 { is_handles_result, .. } = attr_signature_info;

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
                            near_sdk::__private::AbiJsonParameter {
                                name: #arg_name.to_string(),
                                type_schema: #schema,
                            }
                        }),
                        SerializerType::Borsh => params.push(quote! {
                            near_sdk::__private::AbiBorshParameter {
                                name: #arg_name.to_string(),
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

                        let abi_type =
                            generate_abi_type(typ, &attr_signature_info.result_serializer);
                        callback_vec = Some(quote! { Some(#abi_type) })
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
                near_sdk::__private::AbiParameters::Json {
                    args: vec![#(#params),*]
                }
            },
            SerializerType::Borsh => quote! {
                near_sdk::__private::AbiParameters::Borsh {
                    args: vec![#(#params),*]
                }
            },
        };
        let callback_vec = callback_vec.unwrap_or(quote! { None });

        let result = match attr_signature_info.method_type {
            MethodType::Init | MethodType::InitIgnoreState => {
                // Init methods must return the contract state, so the return type does not matter
                quote! {
                    None
                }
            }
            _ => match &attr_signature_info.returns {
                ReturnType::Default => {
                    quote! {
                        None
                    }
                }
                ReturnType::Type(_, ty) if is_handles_result && utils::type_is_result(ty) => {
                    let ty = if let Some(ty) = utils::extract_ok_type(ty) {
                        ty
                    } else {
                        return syn::Error::new_spanned(
                            ty,
                            "Function marked with #[handle_result] should have return type Result<T, E> (where E implements FunctionError).",
                        )
                        .into_compile_error();
                    };
                    let abi_type = generate_abi_type(ty, &attr_signature_info.result_serializer);
                    quote! { Some(#abi_type) }
                }
                ReturnType::Type(_, ty) if is_handles_result => {
                    return syn::Error::new(
                        ty.span(),
                        "Method marked with #[handle_result] should return Result<T, E> (where E implements FunctionError).",
                    )
                    .to_compile_error();
                }
                ReturnType::Type(_, ty) => {
                    let abi_type = generate_abi_type(ty, &attr_signature_info.result_serializer);
                    quote! { Some(#abi_type) }
                }
            },
        };

        quote! {
             near_sdk::__private::AbiFunction {
                 name: #function_name_str.to_string(),
                 doc: #function_doc,
                 kind: #kind,
                 modifiers: #modifiers,
                 params: #params,
                 callbacks: vec![#(#callbacks),*],
                 callbacks_vec: #callback_vec,
                 result: #result
             }
        }
    }
}

fn generate_schema(ty: &Type, serializer_type: &SerializerType) -> TokenStream2 {
    match serializer_type {
        SerializerType::JSON => quote! {
            gen.subschema_for::<#ty>()
        },
        SerializerType::Borsh => quote! {
            <#ty as near_sdk::borsh::BorshSchema>::schema_container()
        },
    }
}

fn generate_abi_type(ty: &Type, serializer_type: &SerializerType) -> TokenStream2 {
    let schema = generate_schema(&ty, serializer_type);
    match serializer_type {
        SerializerType::JSON => quote! {
            near_sdk::__private::AbiType::Json {
                type_schema: #schema,
            }
        },
        SerializerType::Borsh => quote! {
            near_sdk::__private::AbiType::Borsh {
                type_schema: #schema,
            }
        },
    }
}

pub fn parse_rustdoc(attrs: &[Attribute]) -> Option<String> {
    let doc = attrs
        .iter()
        .filter_map(|attr| {
            if attr.path.is_ident("doc") {
                if let NameValue(MetaNameValue { lit: Str(s), .. }) = attr.parse_meta().ok()? {
                    Some(s.value())
                } else {
                    None
                }
            } else {
                None
            }
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
    use quote::quote;
    use syn::{parse_quote, Type};
    use crate::core_impl::ImplItemMethodInfo;

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
        
        let expected = quote! {
            near_sdk::__private::AbiFunction {
                name: "f3".to_string(),
                doc: Some(" I am a function.".to_string()),
                kind: near_sdk::__private::AbiFunctionKind::Call,
                modifiers: vec![],
                params: near_sdk::__private::AbiParameters::Json {
                    args: vec![
                        near_sdk::__private::AbiJsonParameter {
                            name: "arg0".to_string(),
                            type_schema: gen.subschema_for::<FancyStruct>(),
                        },
                        near_sdk::__private::AbiJsonParameter {
                            name: "arg1".to_string(),
                            type_schema: gen.subschema_for::<u64>(),
                        }
                    ]
                },
                callbacks: vec![],
                callbacks_vec: None,
                result: Some(near_sdk::__private::AbiType::Json {
                    type_schema: gen.subschema_for::<IsOk>(),
                })
            }
        };
        
        assert_eq!(actual.to_string(), expected.to_string());
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

        let expected = quote! {
            near_sdk::__private::AbiFunction {
                name: "f3".to_string(),
                doc: None,
                kind: near_sdk::__private::AbiFunctionKind::Call,
                modifiers: vec![near_sdk::__private::AbiFunctionModifier::Payable],
                params: near_sdk::__private::AbiParameters::Borsh {
                    args: vec![
                        near_sdk::__private::AbiBorshParameter {
                            name: "arg0".to_string(),
                            type_schema: <FancyStruct as near_sdk::borsh::BorshSchema>::schema_container(),
                        }
                    ]
                },
                callbacks: vec![],
                callbacks_vec: None,
                result: Some(near_sdk::__private::AbiType::Borsh {
                    type_schema: <IsOk as near_sdk::borsh::BorshSchema>::schema_container(),
                })
            }
        };

        assert_eq!(actual.to_string(), expected.to_string());
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
        
        let expected = quote! {
           near_sdk::__private::AbiFunction { 
                name: "method".to_string(),
                doc: None, 
                kind: near_sdk::__private::AbiFunctionKind::View , 
                modifiers: vec! [near_sdk::__private::AbiFunctionModifier::Private],
                params: near_sdk::__private::AbiParameters::Json { 
                    args: vec![]
                }, 
                callbacks: vec! [], 
                callbacks_vec: Some(near_sdk::__private::AbiType::Json { 
                    type_schema: gen.subschema_for::< String >() , 
                }),
                result: Some(near_sdk::__private::AbiType::Json {
                    type_schema: gen.subschema_for::< bool >() ,
                })
            }
        };
        
        assert_eq!(actual.to_string(), expected.to_string());
    }
    
    #[test]
    fn test_generate_abi_callback_args() {
        let impl_type: Type = syn::parse_str("Test").unwrap();
        let mut method = parse_quote! {
            pub fn method(&self, #[callback_unwrap] #[serializer(borsh)] x: &mut u64, #[serializer(borsh)] y: String, #[callback_unwrap] #[serializer(json)] z: Vec<u8>) { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.abi_struct();

        let expected = quote! {
           near_sdk::__private::AbiFunction { 
                name: "method".to_string(),
                doc: None, 
                kind: near_sdk::__private::AbiFunctionKind::View , 
                modifiers: vec! [],
                params: near_sdk::__private::AbiParameters::Borsh {
                    args: vec! [
                        near_sdk::__private::AbiBorshParameter {
                            name: "y".to_string(),
                            type_schema: < String as near_sdk::borsh::BorshSchema >::schema_container(),
                        }
                    ]
                }, 
                callbacks: vec! [
                    near_sdk::__private::AbiType::Borsh { 
                        type_schema: <u64 as near_sdk::borsh::BorshSchema>::schema_container(),
                    },
                    near_sdk::__private::AbiType::Json {
                        type_schema: gen.subschema_for::< Vec<u8> >(),
                    }
                ],
                callbacks_vec: None,
                result: None 
            }
        };

        assert_eq!(actual.to_string(), expected.to_string());
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

        let expected = quote! {
            near_sdk::__private::AbiFunction {
                name: "new".to_string(),
                doc: None,
                kind: near_sdk::__private::AbiFunctionKind::Call,
                modifiers: vec![
                    near_sdk::__private::AbiFunctionModifier::Init
                ],
                params: near_sdk::__private::AbiParameters::Json {
                    args: vec![]
                },
                callbacks: vec![],
                callbacks_vec: None,
                result: None
            }
        };

        assert_eq!(actual.to_string(), expected.to_string());
    }
    
    #[test]
    fn test_generate_abi_no_return() {
        let impl_type: Type = syn::parse_str("Test").unwrap();
        let mut method = parse_quote! {
            pub fn method() { }
        };
        let method_info = ImplItemMethodInfo::new(&mut method, false, impl_type).unwrap().unwrap();
        let actual = method_info.abi_struct();

        let expected = quote! {
            near_sdk::__private::AbiFunction {
                name: "method".to_string(),
                doc: None,
                kind: near_sdk::__private::AbiFunctionKind::View,
                modifiers: vec![],
                params: near_sdk::__private::AbiParameters::Json {
                    args: vec![]
                },
                callbacks: vec![],
                callbacks_vec: None,
                result: None
            }
        };

        assert_eq!(actual.to_string(), expected.to_string());
    }
}
