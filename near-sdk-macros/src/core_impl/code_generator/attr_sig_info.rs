use proc_macro2::TokenStream as TokenStream2;

use crate::core_impl::info_extractor::{ArgInfo, AttrSigInfo, BindgenArgType, SerializerType};
use crate::core_impl::{utils, MethodKind};
use quote::quote;

impl AttrSigInfo {
    /// Whether the signature has function arguments.
    pub fn has_input_args(&self) -> bool {
        self.input_args().next().is_some()
    }

    /// Whether the method has `payable` attribute.
    /// Only available when `__abi-generate` feature is enabled as it's only in the abi generator
    /// currently.
    #[cfg(feature = "__abi-generate")]
    pub fn is_payable(&self) -> bool {
        use MethodKind::*;

        match &self.method_kind {
            Call(call_method) => call_method.is_payable,
            Init(init_method) => init_method.is_payable,
            View(_) => false,
        }
    }

    /// Whether the method has `private` attribute.
    pub fn is_private(&self) -> bool {
        use MethodKind::*;

        match &self.method_kind {
            Call(call_method) => call_method.is_private,
            Init(_) => false,
            View(view_method) => view_method.is_private,
        }
    }

    pub fn input_struct_ser(&self) -> TokenStream2 {
        let args: Vec<_> = self.input_args().collect();
        assert!(
            !args.is_empty(),
            "Can only generate input struct for when input args are specified"
        );
        let attribute = match &self.input_serializer {
            SerializerType::JSON => quote! {
                #[derive(::near_sdk::serde::Serialize)]
                #[serde(crate = "::near_sdk::serde")]
            },
            SerializerType::Borsh => quote! {
                #[derive(::near_sdk::borsh::BorshSerialize)]
                #[borsh(crate = "::near_sdk::borsh")]
            },
        };
        let mut fields = TokenStream2::new();
        for arg in args {
            let ArgInfo { ty, ident, .. } = &arg;
            fields.extend(quote! {
                #ident: &'nearinput #ty,
            });
        }
        quote! {
            #attribute
            struct Input<'nearinput> {
                #fields
            }
        }
    }
    /// Create struct representing input arguments to deserialize.
    ///
    /// Code generated is based on the serialization type of `Self::input_serializer`.
    ///
    /// Each argument is getting converted to a field in a struct. Specifically argument:
    /// `ATTRIBUTES ref mut binding @ SUBPATTERN : TYPE` is getting converted to:
    /// `binding: SUBTYPE,` where `TYPE` is one of the following: `& SUBTYPE`, `&mut SUBTYPE`,
    /// and `SUBTYPE` is one of the following: `[T; n]`, path like
    /// `std::collections::HashMap<SUBTYPE, SUBTYPE>`, or tuple `(SUBTYPE0, SUBTYPE1, ...)`.
    /// # Example
    /// ```ignore
    /// struct Input {
    ///   arg0: Vec<String>,
    ///   arg1: [u64; 10],
    ///   arg2: (u64, Vec<String>),
    /// }
    /// ```
    pub fn input_struct_deser(&self) -> TokenStream2 {
        let args: Vec<_> = self.input_args().collect();
        assert!(
            !args.is_empty(),
            "Can only generate input struct for when input args are specified"
        );
        let attribute = match &self.input_serializer {
            SerializerType::JSON => quote! {
                #[derive(::near_sdk::serde::Deserialize)]
                #[serde(crate = "::near_sdk::serde")]
            },
            SerializerType::Borsh => quote! {
                #[derive(::near_sdk::borsh::BorshDeserialize)]
                #[borsh(crate = "::near_sdk::borsh")]
            },
        };
        let mut fields = TokenStream2::new();
        for arg in args {
            let ArgInfo { ty, ident, .. } = &arg;
            fields.extend(quote! {
                #ident: #ty,
            });
        }
        quote! {
            #attribute
            struct Input {
                #fields
            }
        }
    }

    /// Create pattern that decomposes input struct using correct mutability modifiers.
    /// # Example:
    /// ```ignore
    /// Input {
    ///     arg0,
    ///     mut arg1,
    ///     arg2
    /// }
    /// ```
    pub fn decomposition_pattern(&self) -> TokenStream2 {
        let args: Vec<_> = self.input_args().collect();
        assert!(
            !args.is_empty(),
            "Can only generate decomposition pattern for when input args are specified."
        );
        let mut fields = TokenStream2::new();
        for arg in args {
            let ArgInfo { mutability, ident, .. } = &arg;
            fields.extend(quote! {
            #mutability #ident,
            });
        }
        quote! {
            Input {
                #fields
            }
        }
    }

    /// Create expression that constructs the struct with references to each variable.
    /// # Example:
    /// ```ignore
    /// Input {
    ///     arg0: &arg0,
    ///     arg1: &arg1,
    ///     arg2: &arg2,
    /// }
    /// ```
    pub fn constructor_expr_ref(&self) -> TokenStream2 {
        let args: Vec<_> = self.input_args().collect();
        assert!(
            !args.is_empty(),
            "Can only generate constructor expression for when input args are specified."
        );
        let mut fields = TokenStream2::new();
        for arg in args {
            let ArgInfo { ident, .. } = &arg;
            fields.extend(quote! {
                #ident: &#ident,
            });
        }
        quote! {
            Input {
                #fields
            }
        }
    }

    /// Create a sequence of arguments that can be used to call the method or the function
    /// of the smart contract.
    ///
    /// # Example:
    /// ```ignore
    /// a, &b, &mut c,
    /// ```
    pub fn arg_list(&self) -> TokenStream2 {
        let mut result = TokenStream2::new();
        for arg in &self.args {
            let ArgInfo { reference, mutability, ident, .. } = &arg;
            result.extend(quote! {
                #reference #mutability #ident,
            });
        }
        result
    }

    /// Create a sequence of patterns and types to be used in the method signature.
    ///
    /// # Example:
    /// ```ignore
    /// a: u64, b: &mut T, ref mut c: Vec<String>,
    /// ```
    pub fn pat_type_list(&self) -> TokenStream2 {
        let mut result = TokenStream2::new();
        for arg in self.input_args() {
            let ArgInfo { original, .. } = &arg;
            result.extend(quote! {
                #original,
            });
        }
        result
    }

    /// Create code that deserializes arguments that were decorated with `#[callback*]`
    pub fn callback_deserialization(&self) -> TokenStream2 {
        self.args
            .iter()
            .filter(|arg| {
                matches!(
                    arg.bindgen_ty,
                    BindgenArgType::CallbackArg | BindgenArgType::CallbackResultArg
                )
            })
            .enumerate()
            .fold(TokenStream2::new(), |acc, (idx, arg)| {
                let idx = idx as u64;
                let ArgInfo { mutability, ident, ty, bindgen_ty, serializer_ty, .. } = arg;
                match &bindgen_ty {
                    BindgenArgType::CallbackArg => {
                        let read_data = quote! {
                            let data: ::std::vec::Vec<u8> = match ::near_sdk::env::promise_result(#idx) {
                                ::near_sdk::PromiseResult::Successful(x) => x,
                                _ => ::near_sdk::env::panic_err(::near_sdk::errors::CallbackComputationUnsuccessful::new(#idx).into()),
                            };
                        };
                        let invocation = deserialize_data(serializer_ty);
                        quote! {
                            #acc
                            #read_data
                            let #mutability #ident: #ty = #invocation;
                        }
                    }
                    BindgenArgType::CallbackResultArg => {
                        let ok_type = if let Some(ok_type) = utils::extract_ok_type(ty) {
                            ok_type
                        } else {
                            return syn::Error::new_spanned(ty, "Function parameters marked with \
                                #[callback_result] should have type Result<T, PromiseError>").into_compile_error()
                        };
                        let deserialize = deserialize_data(serializer_ty);
                        let deserialization_branch = match ok_type {
                            // The unit type in this context is a bit special because functions
                            // without an explicit return type do not serialize their response.
                            // But when someone tries to refer to their callback result with
                            // `#[callback_result]` they specify the callback type as
                            // `Result<(), PromiseError>` which cannot be correctly deserialized from
                            // an empty byte array.
                            //
                            // So instead of going through serde, we consider deserialization to be
                            // successful if the byte array is empty or try the normal
                            // deserialization otherwise.
                            syn::Type::Tuple(type_tuple) if type_tuple.elems.is_empty() =>
                                quote! {
                                    ::near_sdk::PromiseResult::Successful(data) if data.is_empty() =>
                                        ::std::result::Result::Ok(()),
                                    ::near_sdk::PromiseResult::Successful(data) => ::std::result::Result::Ok(#deserialize)
                                },
                            _ =>
                                quote! {
                                    ::near_sdk::PromiseResult::Successful(data) => ::std::result::Result::Ok(#deserialize)
                                }
                        };
                        let result = quote! {
                            match ::near_sdk::env::promise_result(#idx) {
                                #deserialization_branch,
                                ::near_sdk::PromiseResult::Failed => ::std::result::Result::Err(::near_sdk::PromiseError::Failed),
                            }
                        };
                        quote! {
                            #acc
                            let #mutability #ident: #ty = #result;
                        }
                    }
                    _ => unreachable!()
                }
            })
    }

    /// Create code that deserializes arguments that were decorated with `#[callback_vec]`.
    pub fn callback_vec_deserialization(&self) -> TokenStream2 {
        self
            .args
            .iter()
            .filter(|arg| matches!(arg.bindgen_ty, BindgenArgType::CallbackArgVec))
            .fold(TokenStream2::new(), |acc, arg| {
                let ArgInfo { mutability, ident, ty, .. } = arg;
                let invocation = deserialize_data(&arg.serializer_ty);
                quote! {
                    #acc
                    let #mutability #ident: #ty = ::std::iter::Iterator::collect(::std::iter::Iterator::map(
                        0..::near_sdk::env::promise_results_count(),
                        |i| {
                            let data: ::std::vec::Vec<u8> = match ::near_sdk::env::promise_result(i) {
                                ::near_sdk::PromiseResult::Successful(x) => x,
                                _ => ::near_sdk::env::panic_err(::near_sdk::errors::CallbackComputationUnsuccessful::new(i).into()),
                            };
                            #invocation
                        }));
                }
            })
    }
}

fn deserialize_data(ty: &SerializerType) -> TokenStream2 {
    match ty {
        SerializerType::JSON => quote! {
            ::near_sdk::serde_json::from_slice(&data).expect("Failed to deserialize callback using JSON")
        },
        SerializerType::Borsh => quote! {
            ::near_sdk::borsh::BorshDeserialize::try_from_slice(&data).expect("Failed to deserialize callback using Borsh")
        },
    }
}
