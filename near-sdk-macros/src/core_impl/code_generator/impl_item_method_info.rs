use crate::core_impl::info_extractor::{ImplItemMethodInfo, SerializerType};
use crate::core_impl::utils;
use crate::core_impl::{MethodKind, ReturnKind};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Receiver;

impl ImplItemMethodInfo {
    /// Generate wrapper method for the given method of the contract.
    pub fn method_wrapper(&self) -> TokenStream2 {
        let non_bindgen_attrs = self.non_bindgen_attrs_tokens();

        let ident = &self.attr_signature_info.ident;

        let panic_hook = self.panic_hook_tokens();

        let arg_struct = self.arg_struct_tokens();
        let arg_parsing = self.arg_parsing_tokens();

        let callback_deser = self.attr_signature_info.callback_deserialization();
        let callback_vec_deser = self.attr_signature_info.callback_vec_deserialization();

        let deposit_check = self.deposit_check_tokens();
        let is_private_check = self.private_check_tokens();
        let state_check = self.state_check_tokens();

        let body = match self.attr_signature_info.returns.kind {
            // Extractor errors if Init method doesn't return anything, so we don't need extra check
            // here.
            ReturnKind::Default => self.void_return_body_tokens(),
            ReturnKind::General(_) => self.value_return_body_tokens(),
            ReturnKind::HandlesResultExplicit { .. } => self.result_return_body_tokens(),
            ReturnKind::HandlesResultImplicit { .. } => self.result_return_body_tokens(),
        };

        quote! {
            #non_bindgen_attrs
            #[cfg(target_arch = "wasm32")]
            #[no_mangle]
            pub extern "C" fn #ident() {
                #panic_hook
                #is_private_check
                #deposit_check
                #arg_struct
                #arg_parsing
                #callback_deser
                #callback_vec_deser
                #state_check
                #body
            }
        }
    }

    fn void_return_body_tokens(&self) -> TokenStream2 {
        let contract_init = self.contract_init_tokens();
        let method_invocation = self.method_invocation_tokens();
        let contract_ser = self.contract_ser_tokens();

        quote! {
            #contract_init
            #method_invocation;
            #contract_ser
        }
    }

    fn value_return_body_tokens(&self) -> TokenStream2 {
        let contract_init = self.contract_init_tokens();
        let method_invocation_with_return = self.method_invocation_with_return_tokens();
        let contract_ser = self.contract_ser_tokens();
        let value_ser = self.value_ser_tokens();
        let value_return = self.value_return_tokens();

        quote! {
            #contract_init
            #method_invocation_with_return
            #value_ser
            #value_return
            #contract_ser
        }
    }

    fn result_return_body_tokens(&self) -> TokenStream2 {
        let contract_init = self.contract_init_tokens();
        let method_invocation_with_return = self.method_invocation_with_return_tokens();
        let contract_ser = self.contract_ser_tokens();
        let value_ser = self.value_ser_tokens();
        let value_return = self.value_return_tokens();
        let result_identifier = self.result_identifier();
        let handle_error = self.error_handling_tokens();

        quote! {
            #contract_init
            #method_invocation_with_return
            match #result_identifier {
                ::std::result::Result::Ok(#result_identifier) => {
                    #value_ser
                    #value_return
                    #contract_ser
                }
                ::std::result::Result::Err(err) => {
                    #handle_error
                }
            }
        }
    }

    fn error_handling_tokens(&self) -> TokenStream2 {
        match &self.attr_signature_info.returns.kind {
            ReturnKind::HandlesResultImplicit(status_result) => {
                if status_result.persist_on_error {
                    let error_method_name =
                        quote::format_ident!("{}_error", self.attr_signature_info.ident);
                    let contract_ser = self.contract_ser_tokens();
                    quote! {
                        #contract_ser
                        let promise = Contract::ext(::near_sdk::env::current_account_id()).#error_method_name(err).as_return();
                    }
                } else {
                    utils::standardized_error_panic_tokens()
                }
            }
            ReturnKind::HandlesResultExplicit { .. } => quote! {
                ::near_sdk::FunctionError::panic(&err);
            },
            _ => quote! {},
        }
    }

    fn panic_hook_tokens(&self) -> TokenStream2 {
        quote! {
            ::near_sdk::env::setup_panic_hook();
        }
    }

    fn arg_struct_tokens(&self) -> TokenStream2 {
        if self.attr_signature_info.has_input_args() {
            self.attr_signature_info.input_struct_deser()
        } else {
            quote! {}
        }
    }

    fn arg_parsing_tokens(&self) -> TokenStream2 {
        if self.attr_signature_info.has_input_args() {
            let decomposition = self.attr_signature_info.decomposition_pattern();
            let serializer_invocation = match self.attr_signature_info.input_serializer {
                SerializerType::JSON => quote! {
                    ::near_sdk::serde_json::from_slice(
                        &::near_sdk::env::input().expect("Expected input since method has arguments.")
                    ).expect("Failed to deserialize input from JSON.")
                },
                SerializerType::Borsh => quote! {
                    ::near_sdk::borsh::BorshDeserialize::try_from_slice(
                        &::near_sdk::env::input().expect("Expected input since method has arguments.")
                    ).expect("Failed to deserialize input from Borsh.")
                },
            };
            quote! {
                let #decomposition : Input = #serializer_invocation ;
            }
        } else {
            quote! {}
        }
    }

    fn deposit_check_tokens(&self) -> TokenStream2 {
        use MethodKind::*;

        let reject_deposit_code = || {
            // If method is not payable, do a check to make sure that it doesn't consume deposit
            let method_name = &self.attr_signature_info.ident.to_string();
            quote! {
                if ::near_sdk::env::attached_deposit().as_yoctonear() != 0 {
                    ::near_sdk::env::panic_err(::near_sdk::errors::DepositNotAccepted::new(#method_name).into());
                }
            }
        };

        match &self.attr_signature_info.method_kind {
            Call(call_method) => {
                if !call_method.is_payable {
                    reject_deposit_code()
                } else {
                    quote! {}
                }
            }

            Init(init_method) => {
                if !init_method.is_payable {
                    reject_deposit_code()
                } else {
                    quote! {}
                }
            }

            View(_) => quote! {},
        }
    }

    fn private_check_tokens(&self) -> TokenStream2 {
        if self.attr_signature_info.is_private() {
            let method_name = &self.attr_signature_info.ident.to_string();
            quote! {
                if ::near_sdk::env::current_account_id() != ::near_sdk::env::predecessor_account_id() {
                    ::near_sdk::errors::PrivateMethod::new(#method_name);
                }
            }
        } else {
            quote! {}
        }
    }

    fn state_check_tokens(&self) -> TokenStream2 {
        use MethodKind::*;

        // The purpose of the state check is to prevent the contract from being initialized twice,
        // so it's not applicable to Call and View methods.
        match &self.attr_signature_info.method_kind {
            Call(_) => quote! {},

            Init(init_method) => {
                if !init_method.ignores_state {
                    quote! {
                        if ::near_sdk::env::state_exists() {
                            ::near_sdk::env::panic_err(::near_sdk::errors::ContractAlreadyInitialized{}.into());
                        }
                    }
                } else {
                    quote! {}
                }
            }

            View(_) => quote! {},
        }
    }

    fn contract_init_tokens(&self) -> TokenStream2 {
        use MethodKind::*;

        let struct_type = &self.struct_type;
        let ident = &self.attr_signature_info.ident;
        let arg_list = self.attr_signature_info.arg_list();

        let contract_deser = |receiver: &Receiver| {
            let mutability = receiver.mutability;

            quote! {
                let #mutability contract: #struct_type = ::near_sdk::env::state_read().unwrap_or_default();
            }
        };

        // In Call and View methods, the contract is deserialized from the state.
        // In Init methods the contract is created with the constructor.
        match &self.attr_signature_info.method_kind {
            Call(call_method) => {
                if let Some(receiver) = &call_method.receiver {
                    contract_deser(receiver)
                } else {
                    quote! {}
                }
            }

            Init(_) => quote! {
                let contract = #struct_type::#ident(#arg_list);
            },

            View(view_method) => {
                if let Some(receiver) = &view_method.receiver {
                    contract_deser(receiver)
                } else {
                    quote! {}
                }
            }
        }
    }

    fn contract_ser_tokens(&self) -> TokenStream2 {
        use MethodKind::*;

        fn contract_ser() -> TokenStream2 {
            quote! {
                ::near_sdk::env::state_write(&contract);
            }
        }

        match &self.attr_signature_info.method_kind {
            Call(call_method) => {
                if call_method.receiver.is_some() {
                    contract_ser()
                } else {
                    quote! {}
                }
            }

            Init(_) => contract_ser(),

            // View methods don't update the state.
            View(_) => quote! {},
        }
    }

    fn method_invocation_tokens(&self) -> TokenStream2 {
        use MethodKind::*;

        let ident = &self.attr_signature_info.ident;
        let arg_list = self.attr_signature_info.arg_list();

        let method_invocation = || {
            quote! {
                contract.#ident(#arg_list)
            }
        };

        let static_invocation = || {
            let struct_type = &self.struct_type;
            quote! {
                #struct_type::#ident(#arg_list)
            }
        };

        match &self.attr_signature_info.method_kind {
            Call(call_method) => {
                if call_method.receiver.is_some() {
                    method_invocation()
                } else {
                    static_invocation()
                }
            }

            // The method invocation in Init methods is done in contract initialization.
            Init(_) => quote! {},

            View(view_method) => {
                if view_method.receiver.is_some() {
                    method_invocation()
                } else {
                    static_invocation()
                }
            }
        }
    }

    fn method_invocation_with_return_tokens(&self) -> TokenStream2 {
        use MethodKind::*;

        let method_invocation = self.method_invocation_tokens();

        match &self.attr_signature_info.method_kind {
            Call(_) => quote! {
                let result = #method_invocation;
            },

            // The method invocation in Init methods is done in contract initialization.
            Init(_) => quote! {},

            View(_) => quote! {
                let result = #method_invocation;
            },
        }
    }

    fn value_ser_tokens(&self) -> TokenStream2 {
        use MethodKind::*;

        let value_ser = |result_serializer: &SerializerType| match result_serializer {
            SerializerType::JSON => quote! {
                let result = ::near_sdk::serde_json::to_vec(&result).expect("Failed to serialize the return value using JSON.");
            },
            SerializerType::Borsh => quote! {
                let result = ::near_sdk::borsh::to_vec(&result).expect("Failed to serialize the return value using Borsh.");
            },
        };

        match &self.attr_signature_info.method_kind {
            Call(call_method) => value_ser(&call_method.result_serializer),

            // There is no value returned on init, only the newly created contract is written to the
            // state.
            Init(_) => quote! {},

            View(view_method) => value_ser(&view_method.result_serializer),
        }
    }

    fn value_return_tokens(&self) -> TokenStream2 {
        use MethodKind::*;

        let value_return = || {
            quote! {
                ::near_sdk::env::value_return(&result);
            }
        };

        match &self.attr_signature_info.method_kind {
            Call(_) => value_return(),

            // There is no value returned on init, only the newly created contract is written to the
            // state.
            Init(_) => quote! {},

            View(_) => value_return(),
        }
    }

    fn result_identifier(&self) -> TokenStream2 {
        use MethodKind::*;

        match &self.attr_signature_info.method_kind {
            Call(_) => quote! {
                result
            },

            // In Init methods the Result is the contract.
            Init(_) => quote! {
                contract
            },

            View(_) => quote! {
                result
            },
        }
    }

    fn non_bindgen_attrs_tokens(&self) -> TokenStream2 {
        self.attr_signature_info.non_bindgen_attrs.iter().fold(TokenStream2::new(), |acc, value| {
            quote! {
                #acc
                #value
            }
        })
    }
}
