#![recursion_limit = "128"]
extern crate proc_macro;

mod core_impl;

use core_impl::{ext::generate_ext_structs, metadata::generate_contract_metadata_method};

use proc_macro::TokenStream;

use self::core_impl::*;
use darling::ast::NestedMeta;
use darling::{Error, FromMeta};
use proc_macro2::{Ident, Span};
use quote::{quote, ToTokens};
use syn::{parse_quote, Expr, ImplItem, ItemEnum, ItemImpl, ItemStruct, ItemTrait, WhereClause};

#[derive(Debug, Clone)]
struct Serializers {
    vec: Vec<Expr>,
}

impl FromMeta for Serializers {
    fn from_expr(expr: &syn::Expr) -> Result<Self, darling::Error> {
        match expr {
            syn::Expr::Array(expr_array) => Ok(Serializers {
                vec: expr_array
                    .elems
                    .iter()
                    .map(<Expr as FromMeta>::from_expr)
                    .map(|x| x.unwrap())
                    .collect::<Vec<_>>(),
            }),
            _ => Err(Error::unexpected_expr_type(expr)),
        }
    }
}

#[derive(FromMeta)]
struct NearMacroArgs {
    serializers: Option<Serializers>,
    contract_state: Option<bool>,
    contract_metadata: Option<core_impl::ContractMetadata>,
    inside_nearsdk: Option<bool>,
}

/// This attribute macro is used to enhance the near_bindgen macro.
/// It is used to add Borsh and Serde derives for serialization and deserialization.
/// It also adds `BorshSchema` and `JsonSchema` if needed
///
/// If you would like to add Borsh or Serde serialization and deserialization to your contract,
/// you can use the abi attribute and pass in the serializers you would like to use.
///
/// # Example
///
/// ```ignore
/// #[near(serializers=[borsh, json])]
/// struct MyStruct {
///    pub name: String,
/// }
/// ```
/// effectively becomes:
/// ```ignore
/// use borsh::{BorshSerialize, BorshDeserialize};
/// use serde::{Serialize, Deserialize};
/// use near_sdk_macro::NearSchema;
/// #[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, NearSchema)]
/// #[borsh(crate = "near_sdk::borsh")]
/// #[serde(crate = "near_sdk::serde")]
/// struct MyStruct {
///   pub name: String,
/// }
/// ```
/// Please note that `BorshSchema` and `JsonSchema` are added inside NearSchema whenever you use near macro for struct or enum.
/// By default, if no serializers are passed, Borsh is used.
///
/// If you want this struct to be a contract state, you can pass in the contract_state argument.
///
/// # Example
/// ```ignore
/// #[near(contract_state)]
/// struct MyStruct {
///     pub name: String,
/// }
/// ```
/// becomes:
/// ```ignore
/// #[near_bindgen]
/// #[derive(BorshSerialize, BorshDeserialize, NearSchema)]
/// #[borsh(crate = "near_sdk::borsh")]
/// struct MyStruct {
///    pub name: String,
/// }
/// ```
///
/// As well, the macro supports arguments like `event_json` and `contract_metadata`.
///
#[proc_macro_attribute]
pub fn near(attr: TokenStream, item: TokenStream) -> TokenStream {
    if attr.to_string().contains("event_json") {
        return core_impl::near_events(attr, item);
    }

    let meta_list = match NestedMeta::parse_meta_list(attr.into()) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(Error::from(e).write_errors());
        }
    };

    let near_macro_args = match NearMacroArgs::from_list(&meta_list) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };

    let near_sdk_crate = if near_macro_args.inside_nearsdk.unwrap_or(false) {
        quote! {crate}
    } else {
        quote! {::near_sdk}
    };
    let string_borsh_crate = quote! {#near_sdk_crate::borsh}.to_string();
    let string_serde_crate = quote! {#near_sdk_crate::serde}.to_string();

    let mut expanded: proc_macro2::TokenStream = quote! {};

    if near_macro_args.contract_state.unwrap_or(false) {
        if let Some(metadata) = near_macro_args.contract_metadata {
            expanded = quote! {#[#near_sdk_crate::near_bindgen(#metadata)]}
        } else {
            expanded = quote! {#[#near_sdk_crate::near_bindgen]}
        }
    };

    let mut has_borsh = false;
    let mut has_json = false;

    let mut borsh_attr = quote! {};

    match near_macro_args.serializers {
        Some(serializers) => {
            let attr2 = serializers.clone();

            attr2.vec.iter().for_each(|old_expr| {
                let new_expr = &mut old_expr.clone();
                match &mut *new_expr {
                    Expr::Call(ref mut call_expr) => {
                        if let Expr::Path(ref mut path) = &mut *call_expr.func {
                            if let Some(ident) = path.path.get_ident() {
                                if *ident == "json" {
                                    has_json = true;
                                    path.path =
                                        syn::Path::from(Ident::new("serde", Span::call_site()));
                                    call_expr.args.push(parse_quote! {crate=#string_serde_crate});
                                } else if *ident == "borsh" {
                                    has_borsh = true;
                                    call_expr.args.push(parse_quote! {crate=#string_borsh_crate});
                                }
                            }
                        }
                        borsh_attr = quote! {#[#new_expr]};
                    }
                    Expr::Path(ref mut path_expr) => {
                        if let Some(ident) = path_expr.path.get_ident() {
                            if *ident == "json" {
                                has_json = true;
                            }
                            if *ident == "borsh" {
                                has_borsh = true;
                                borsh_attr = quote! {#[borsh(crate=#string_borsh_crate)]};
                            }
                        }
                    }
                    _ => {}
                }
            });
        }
        None => {
            has_borsh = true;
            borsh_attr = quote! {#[borsh(crate = #string_borsh_crate)]};
        }
    }

    #[cfg(feature = "abi")]
    {
        let schema_derive: proc_macro2::TokenStream =
            get_schema_derive(has_json, has_borsh, near_sdk_crate.clone(), false);
        expanded = quote! {
            #expanded
            #schema_derive
        };
    }

    if has_borsh {
        expanded = quote! {
            #expanded
            #[derive(#near_sdk_crate::borsh::BorshSerialize, #near_sdk_crate::borsh::BorshDeserialize)]
            #borsh_attr
        };
    }

    if has_json {
        expanded = quote! {
            #expanded
            #[derive(#near_sdk_crate::serde::Serialize, #near_sdk_crate::serde::Deserialize)]
            #[serde(crate = #string_serde_crate)]
        };
    }

    if let Ok(input) = syn::parse::<ItemStruct>(item.clone()) {
        expanded = quote! {
            #expanded
            #input
        };
    } else if let Ok(input) = syn::parse::<ItemEnum>(item.clone()) {
        expanded = quote! {
            #expanded
            #input
        };
    } else if let Ok(input) = syn::parse::<ItemImpl>(item) {
        expanded = quote! {
            #[#near_sdk_crate::near_bindgen]
            #input
        };
    } else {
        return TokenStream::from(
            syn::Error::new(
                Span::call_site(),
                "near macro can only be used on struct or enum definition and impl sections.",
            )
            .to_compile_error(),
        );
    }

    TokenStream::from(expanded)
}

/// This attribute macro is used on a struct and its implementations
/// to generate the necessary code to expose `pub` methods from the contract as well
/// as generating the glue code to be a valid NEAR contract.
///
/// This macro will generate code to load and deserialize state if the `self` parameter is included
/// as well as saving it back to state if `&mut self` is used.
///
/// For parameter serialization, this macro will generate a struct with all of the parameters as
/// fields and derive deserialization for it. By default this will be JSON deserialized with `serde`
/// but can be overwritten by using `#[serializer(borsh)]`.
///
/// `#[near_bindgen]` will also handle serializing and setting the return value of the
/// function execution based on what type is returned by the function. By default, this will be
/// done through `serde` serialized as JSON, but this can be overwritten using
/// `#[result_serializer(borsh)]`.
///
/// # Examples
///
/// ```ignore
/// use near_sdk::near_bindgen;
///
/// #[near_bindgen]
/// pub struct Contract {
///    data: i8,
/// }
///
/// #[near_bindgen]
/// impl Contract {
///     pub fn some_function(&self) {}
/// }
/// ```
///
/// Events Standard:
///
/// By passing `event_json` as an argument `near_bindgen` will generate the relevant code to format events
/// according to NEP-297
///
/// For parameter serialization, this macro will generate a wrapper struct to include the NEP-297 standard fields `standard` and `version
/// as well as include serialization reformatting to include the `event` and `data` fields automatically.
/// The `standard` and `version` values must be included in the enum and variant declaration (see example below).
/// By default this will be JSON deserialized with `serde`
///
///
/// # Examples
///
/// ```ignore
/// use near_sdk::near_bindgen;
///
/// #[near_bindgen(event_json(standard = "nepXXX"))]
/// pub enum MyEvents {
///    #[event_version("1.0.0")]
///    Swap { token_in: AccountId, token_out: AccountId, amount_in: u128, amount_out: u128 },
///
///    #[event_version("2.0.0")]
///    StringEvent(String),
///
///    #[event_version("3.0.0")]
///    EmptyEvent
/// }
///
/// #[near_bindgen]
/// impl Contract {
///     pub fn some_function(&self) {
///         MyEvents::StringEvent(
///             String::from("some_string")
///         ).emit();
///     }
///
/// }
/// ```
///
/// Contract Source Metadata Standard:
///
/// By using `contract_metadata` as an argument `near_bindgen` will populate the contract metadata
/// according to [`NEP-330`](<https://github.com/near/NEPs/blob/master/neps/nep-0330.md>) standard. This still applies even when `#[near_bindgen]` is used without
/// any arguments.
///
/// All fields(version, link, standard) are optional and will be populated with defaults from the Cargo.toml file if not specified.
///
/// The `contract_source_metadata()` view function will be added and can be used to retrieve the source metadata.
/// Also, the source metadata will be stored as a constant, `CONTRACT_SOURCE_METADATA`, in the contract code.
///
/// # Examples
/// ```ignore
/// use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
/// use near_sdk::near_bindgen;
///
/// #[derive(Default, BorshSerialize, BorshDeserialize)]
/// #[near_bindgen(contract_metadata(
///     version = "39f2d2646f2f60e18ab53337501370dc02a5661c",
///     link = "https://github.com/near-examples/nft-tutorial",
///     standard(standard = "nep330", version = "1.1.0"),
///     standard(standard = "nep171", version = "1.0.0"),
///     standard(standard = "nep177", version = "2.0.0"),
/// ))]
/// struct Contract {}
/// ```
#[proc_macro_attribute]
pub fn near_bindgen(attr: TokenStream, item: TokenStream) -> TokenStream {
    if attr.to_string().contains("event_json") {
        return core_impl::near_events(attr, item);
    }

    let generate_metadata = |ident: &Ident,
                             generics: &syn::Generics|
     -> Result<proc_macro2::TokenStream, proc_macro2::TokenStream> {
        let metadata_impl_gen = generate_contract_metadata_method(ident, generics).into();
        let metadata_impl_gen = syn::parse::<ItemImpl>(metadata_impl_gen)
            .expect("failed to generate contract metadata");
        process_impl_block(metadata_impl_gen)
    };

    if let Ok(input) = syn::parse::<ItemStruct>(item.clone()) {
        let metadata = core_impl::contract_source_metadata_const(attr);

        let metadata_impl_gen = generate_metadata(&input.ident, &input.generics);

        let metadata_impl_gen = match metadata_impl_gen {
            Ok(metadata) => metadata,
            Err(err) => return err.into(),
        };

        let ext_gen = generate_ext_structs(&input.ident, Some(&input.generics));
        #[cfg(feature = "__abi-embed-checked")]
        let abi_embedded = abi::embed();
        #[cfg(not(feature = "__abi-embed-checked"))]
        let abi_embedded = quote! {};
        TokenStream::from(quote! {
            #input
            #ext_gen
            #abi_embedded
            #metadata
            #metadata_impl_gen
        })
    } else if let Ok(input) = syn::parse::<ItemEnum>(item.clone()) {
        let metadata = core_impl::contract_source_metadata_const(attr);
        let metadata_impl_gen = generate_metadata(&input.ident, &input.generics);

        let metadata_impl_gen = match metadata_impl_gen {
            Ok(metadata) => metadata,
            Err(err) => return err.into(),
        };

        let ext_gen = generate_ext_structs(&input.ident, Some(&input.generics));
        #[cfg(feature = "__abi-embed-checked")]
        let abi_embedded = abi::embed();
        #[cfg(not(feature = "__abi-embed-checked"))]
        let abi_embedded = quote! {};

        TokenStream::from(quote! {
            #input
            #ext_gen
            #abi_embedded
            #metadata
            #metadata_impl_gen
        })
    } else if let Ok(input) = syn::parse::<ItemImpl>(item) {
        for method in &input.items {
            if let ImplItem::Fn(m) = method {
                let ident = &m.sig.ident;
                if ident.eq("__contract_abi") || ident.eq("contract_source_metadata") {
                    return TokenStream::from(
                        syn::Error::new_spanned(
                            ident.to_token_stream(),
                            "use of reserved contract method",
                        )
                        .to_compile_error(),
                    );
                }
            }
        }
        match process_impl_block(input) {
            Ok(output) => output,
            Err(output) => output,
        }
        .into()
    } else {
        TokenStream::from(
            syn::Error::new(
                Span::call_site(),
                "near_bindgen can only be used on struct or enum definition and impl sections.",
            )
            .to_compile_error(),
        )
    }
}

// This function deals with impl block processing, generating wrappers and ABI.
//
// # Arguments
// * input - impl block to process.
//
// The Result has a TokenStream error type, because those need to be propagated to the compiler.
fn process_impl_block(
    mut input: ItemImpl,
) -> Result<proc_macro2::TokenStream, proc_macro2::TokenStream> {
    let item_impl_info = match ItemImplInfo::new(&mut input) {
        Ok(x) => x,
        Err(err) => return Err(err.to_compile_error()),
    };

    #[cfg(not(feature = "__abi-generate"))]
    let abi_generated = quote! {};
    #[cfg(feature = "__abi-generate")]
    let abi_generated = abi::generate(&item_impl_info);

    let generated_code = item_impl_info.wrapper_code();

    // Add wrapper methods for ext call API
    let ext_generated_code = item_impl_info.generate_ext_wrapper_code();

    let error_methods = item_impl_info.generate_error_methods();

    Ok(TokenStream::from(quote! {
        #ext_generated_code
        #input
        #generated_code
        #abi_generated
        #error_methods
    })
    .into())
}

/// `ext_contract` takes a Rust Trait and converts it to a module with static methods.
/// Each of these static methods takes positional arguments defined by the Trait,
/// then the receiver_id, the attached deposit and the amount of gas and returns a new Promise.
///
/// # Examples
///
/// ```ignore
/// use near_sdk::ext_contract;
///
/// #[ext_contract(ext_calculator)]
/// trait Calculator {
///     fn mult(&self, a: u64, b: u64) -> u128;
///     fn sum(&self, a: u128, b: u128) -> u128;
/// }
/// ```
#[proc_macro_attribute]
pub fn ext_contract(attr: TokenStream, item: TokenStream) -> TokenStream {
    if let Ok(mut input) = syn::parse::<ItemTrait>(item) {
        let mod_name: Option<proc_macro2::Ident> = if attr.is_empty() {
            None
        } else {
            match syn::parse(attr) {
                Ok(x) => x,
                Err(err) => {
                    return TokenStream::from(
                        syn::Error::new(
                            Span::call_site(),
                            format!("Failed to parse mod name for ext_contract: {}", err),
                        )
                        .to_compile_error(),
                    )
                }
            }
        };
        let item_trait_info = match ItemTraitInfo::new(&mut input, mod_name) {
            Ok(x) => x,
            Err(err) => return TokenStream::from(err.to_compile_error()),
        };
        let ext_api = item_trait_info.wrap_trait_ext();

        TokenStream::from(quote! {
            #input
            #ext_api
        })
    } else {
        TokenStream::from(
            syn::Error::new(Span::call_site(), "ext_contract can only be used on traits")
                .to_compile_error(),
        )
    }
}

// The below attributes a marker-attributes and therefore they are no-op.

/// `callback` is a marker attribute it does not generate code by itself.
#[proc_macro_attribute]
#[deprecated(since = "4.0.0", note = "Case is handled internally by macro, no need to import")]
pub fn callback(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// `callback_args_vec` is a marker attribute it does not generate code by itself.
#[deprecated(since = "4.0.0", note = "Case is handled internally by macro, no need to import")]
#[proc_macro_attribute]
pub fn callback_vec(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// `serializer` is a marker attribute it does not generate code by itself.
#[deprecated(since = "4.0.0", note = "Case is handled internally by macro, no need to import")]
#[proc_macro_attribute]
pub fn serializer(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// `result_serializer` is a marker attribute it does not generate code by itself.
#[deprecated(since = "4.0.0", note = "Case is handled internally by macro, no need to import")]
#[proc_macro_attribute]
pub fn result_serializer(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// `init` is a marker attribute it does not generate code by itself.
#[deprecated(since = "4.0.0", note = "Case is handled internally by macro, no need to import")]
#[proc_macro_attribute]
pub fn init(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[cfg(feature = "abi")]
#[derive(darling::FromDeriveInput, Debug)]
#[darling(attributes(abi), forward_attrs(serde, borsh_skip, schemars, validate))]
struct DeriveNearSchema {
    attrs: Vec<syn::Attribute>,
    json: Option<bool>,
    borsh: Option<bool>,
}

#[proc_macro_derive(NearSchema, attributes(abi, serde, borsh, schemars, validate, inside_nearsdk))]
pub fn derive_near_schema(#[allow(unused)] input: TokenStream) -> TokenStream {
    #[cfg(not(feature = "abi"))]
    {
        TokenStream::from(quote! {})
    }

    #[cfg(feature = "abi")]
    {
        use darling::FromDeriveInput;

        let derive_input = syn::parse_macro_input!(input as syn::DeriveInput);
        let generics = derive_input.generics.clone();
        let args = match DeriveNearSchema::from_derive_input(&derive_input) {
            Ok(v) => v,
            Err(e) => {
                return TokenStream::from(e.write_errors());
            }
        };

        if args.borsh.is_none()
            && args.json.is_none()
            && derive_input.clone().attrs.iter().any(|attr| attr.path().is_ident("abi"))
        {
            return TokenStream::from(
                syn::Error::new_spanned(
                    derive_input.to_token_stream(),
                    "At least one of `json` or `borsh` inside of `#[abi(...)]` must be specified",
                )
                .to_compile_error(),
            );
        }

        // #[abi(json, borsh)]
        let (json_schema, borsh_schema) = (args.json.unwrap_or(false), args.borsh.unwrap_or(false));
        let mut input = derive_input.clone();
        input.attrs = args.attrs;

        let strip_unknown_attr = |attrs: &mut Vec<syn::Attribute>| {
            attrs.retain(|attr| {
                ["serde", "schemars", "validate", "borsh"]
                    .iter()
                    .any(|&path| attr.path().is_ident(path))
            });
        };

        match &mut input.data {
            syn::Data::Struct(data) => {
                for field in &mut data.fields {
                    strip_unknown_attr(&mut field.attrs);
                }
            }
            syn::Data::Enum(data) => {
                for variant in &mut data.variants {
                    strip_unknown_attr(&mut variant.attrs);
                    for field in &mut variant.fields {
                        strip_unknown_attr(&mut field.attrs);
                    }
                }
            }
            syn::Data::Union(_) => {
                return TokenStream::from(
                    syn::Error::new_spanned(
                        input.to_token_stream(),
                        "`NearSchema` does not support derive for unions",
                    )
                    .to_compile_error(),
                )
            }
        }

        let near_sdk_crate =
            if derive_input.attrs.iter().any(|attr| attr.path().is_ident("inside_nearsdk")) {
                quote! {crate}
            } else {
                quote! {::near_sdk}
            };

        // <unspecified> or #[abi(json)]
        let json_schema = json_schema || !borsh_schema;

        let derive = get_schema_derive(json_schema, borsh_schema, near_sdk_crate.clone(), true);

        let input_ident = &input.ident;

        let input_ident_proxy = quote::format_ident!("{}__NEAR_SCHEMA_PROXY", input_ident);

        let json_impl = if json_schema {
            let where_clause = get_where_clause(
                &generics,
                input_ident,
                quote! {#near_sdk_crate::schemars::JsonSchema},
            );
            quote! {
                #[automatically_derived]
                impl #generics #near_sdk_crate::schemars::JsonSchema for #input_ident_proxy #generics #where_clause {
                    fn schema_name() -> ::std::string::String {
                        stringify!(#input_ident #generics).to_string()
                    }

                    fn json_schema(gen: &mut #near_sdk_crate::schemars::gen::SchemaGenerator) -> #near_sdk_crate::schemars::schema::Schema {
                        <#input_ident #generics as #near_sdk_crate::schemars::JsonSchema>::json_schema(gen)
                    }
                }
            }
        } else {
            quote! {}
        };

        let borsh_impl = if borsh_schema {
            let where_clause = get_where_clause(
                &generics,
                input_ident,
                quote! {#near_sdk_crate::borsh::BorshSchema},
            );
            quote! {
                #[automatically_derived]
                impl #generics #near_sdk_crate::borsh::BorshSchema for #input_ident_proxy #generics #where_clause {
                    fn declaration() -> #near_sdk_crate::borsh::schema::Declaration {
                        stringify!(#input_ident #generics).to_string()
                    }

                    fn add_definitions_recursively(
                        definitions: &mut #near_sdk_crate::borsh::__private::maybestd::collections::BTreeMap<
                            #near_sdk_crate::borsh::schema::Declaration,
                            #near_sdk_crate::borsh::schema::Definition
                        >,
                    ) {
                        <#input_ident #generics as #near_sdk_crate::borsh::BorshSchema>::add_definitions_recursively(definitions);
                    }
                }
            }
        } else {
            quote! {}
        };

        TokenStream::from(quote! {
            #[cfg(not(target_arch = "wasm32"))]
            const _: () = {
                #[allow(non_camel_case_types)]
                type #input_ident_proxy #generics = #input_ident #generics;
                {
                    #derive
                    #[allow(dead_code)]
                    #input

                    #json_impl
                    #borsh_impl
                };
            };
        })
    }
}

#[allow(dead_code)]
fn get_schema_derive(
    json_schema: bool,
    borsh_schema: bool,
    near_sdk_crate: proc_macro2::TokenStream,
    need_borsh_crate: bool,
) -> proc_macro2::TokenStream {
    let string_borsh_crate = quote! {#near_sdk_crate::borsh}.to_string();
    let string_schemars_crate = quote! {#near_sdk_crate::schemars}.to_string();

    let mut derive = quote! {};
    if borsh_schema {
        derive = quote! {
            #[cfg_attr(not(target_arch = "wasm32"), derive(#near_sdk_crate::borsh::BorshSchema))]
        };
        if need_borsh_crate {
            derive = quote! {
                #derive
                #[cfg_attr(not(target_arch = "wasm32"), borsh(crate = #string_borsh_crate))]
            };
        }
    }
    if json_schema {
        derive = quote! {
            #derive
            #[cfg_attr(not(target_arch = "wasm32"), derive(#near_sdk_crate::schemars::JsonSchema))]
            #[cfg_attr(not(target_arch = "wasm32"), schemars(crate = #string_schemars_crate))]
        };
    }
    derive
}

#[cfg(feature = "abi")]
fn get_where_clause(
    generics: &syn::Generics,
    input_ident: &syn::Ident,
    trait_name: proc_macro2::TokenStream,
) -> WhereClause {
    let (_, ty_generics, where_clause) = generics.split_for_impl();

    let predicate = parse_quote!(#input_ident #ty_generics: #trait_name);

    let where_clause: WhereClause = if let Some(mut w) = where_clause.cloned() {
        w.predicates.push(predicate);
        w
    } else {
        parse_quote!(where #predicate)
    };
    where_clause
}

/// `PanicOnDefault` generates implementation for `Default` trait that panics with the following
/// message `The contract is not initialized` when `default()` is called.
/// This is a helpful macro in case the contract is required to be initialized with either `init` or
/// `init(ignore_state)`.
#[proc_macro_derive(PanicOnDefault)]
pub fn derive_no_default(item: TokenStream) -> TokenStream {
    if let Ok(input) = syn::parse::<ItemStruct>(item) {
        let name = &input.ident;
        TokenStream::from(quote! {
            impl ::std::default::Default for #name {
                fn default() -> Self {
                    ::near_sdk::env::panic_err(::near_sdk::errors::ContractNotInitialized{}.into());
                }
            }
        })
    } else {
        TokenStream::from(
            syn::Error::new(
                Span::call_site(),
                "PanicOnDefault can only be used on type declarations sections.",
            )
            .to_compile_error(),
        )
    }
}

/// `BorshStorageKey` generates implementation for `BorshIntoStorageKey` trait.
/// It allows the type to be passed as a unique prefix for persistent collections.
/// The type should also implement or derive `BorshSerialize` trait.
#[proc_macro_derive(BorshStorageKey)]
pub fn borsh_storage_key(item: TokenStream) -> TokenStream {
    let (name, generics) = if let Ok(input) = syn::parse::<ItemEnum>(item.clone()) {
        (input.ident, input.generics)
    } else if let Ok(input) = syn::parse::<ItemStruct>(item) {
        (input.ident, input.generics)
    } else {
        return TokenStream::from(
            syn::Error::new(
                Span::call_site(),
                "BorshStorageKey can only be used as a derive on enums or structs.",
            )
            .to_compile_error(),
        );
    };
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicate = parse_quote!(#name #ty_generics: ::near_sdk::borsh::BorshSerialize);
    let where_clause: WhereClause = if let Some(mut w) = where_clause.cloned() {
        w.predicates.push(predicate);
        w
    } else {
        parse_quote!(where #predicate)
    };
    TokenStream::from(quote! {
        impl #impl_generics ::near_sdk::__private::BorshIntoStorageKey for #name #ty_generics #where_clause {}
    })
}

/// `FunctionError` generates implementation for `near_sdk::FunctionError` trait.
/// It allows contract runtime to panic with the type using its `ToString` implementation
/// as the message.
#[proc_macro_derive(FunctionError)]
pub fn function_error(item: TokenStream) -> TokenStream {
    let name = if let Ok(input) = syn::parse::<ItemEnum>(item.clone()) {
        input.ident
    } else if let Ok(input) = syn::parse::<ItemStruct>(item) {
        input.ident
    } else {
        return TokenStream::from(
            syn::Error::new(
                Span::call_site(),
                "FunctionError can only be used as a derive on enums or structs.",
            )
            .to_compile_error(),
        );
    };
    TokenStream::from(quote! {
        impl ::near_sdk::FunctionError for #name {
            fn panic(&self) -> ! {
                ::near_sdk::env::panic_err(::near_sdk::errors::ContractError::new(&::std::string::ToString::to_string(&self)).into())
            }
        }
    })
}

/// NOTE: This is an internal implementation for `#[near_bindgen(events(standard = ...))]` attribute.
///
/// This derive macro is used to inject the necessary wrapper and logic to auto format
/// standard event logs. The other appropriate attribute macros are not injected with this macro.
/// Required attributes below:
/// ```ignore
/// #[derive(near_sdk::serde::Serialize, std::clone::Clone)]
/// #[serde(crate="near_sdk::serde")]
/// #[serde(tag = "event", content = "data")]
/// #[serde(rename_all="snake_case")]
/// pub enum MyEvent {
///     Event
/// }
/// ```
#[proc_macro_derive(EventMetadata, attributes(event_version))]
pub fn derive_event_attributes(item: TokenStream) -> TokenStream {
    if let Ok(input) = syn::parse::<ItemEnum>(item) {
        let name = &input.ident;
        // get `standard` const injected from `near_events`
        let standard_name = format!("{}_event_standard", name);
        let standard_ident = syn::Ident::new(&standard_name, Span::call_site());
        // version from each attribute macro
        let mut event_meta: Vec<proc_macro2::TokenStream> = vec![];
        for var in &input.variants {
            if let Some(version) = core_impl::get_event_version(var) {
                let var_ident = &var.ident;
                event_meta.push(quote! {
                    #name::#var_ident { .. } => {(::std::string::ToString::to_string(&#standard_ident), ::std::string::ToString::to_string(#version))}
                })
            } else {
                return TokenStream::from(
                    syn::Error::new(
                        Span::call_site(),
                        "Near events must have `event_version`. Must have a single string literal value.",
                    )
                    .to_compile_error(),
                );
            }
        }

        // handle lifetimes, generics, and where clauses
        let (impl_generics, type_generics, where_clause) = &input.generics.split_for_impl();
        // add `'near_event` lifetime for user defined events
        let mut generics = input.generics.clone();
        let event_lifetime = syn::Lifetime::new("'near_event", Span::call_site());
        generics.params.insert(
            0,
            syn::GenericParam::Lifetime(syn::LifetimeParam::new(event_lifetime.clone())),
        );
        let (custom_impl_generics, ..) = generics.split_for_impl();

        TokenStream::from(quote! {
            impl #impl_generics #name #type_generics #where_clause {
                pub fn emit(&self) {
                    use ::std::string::String;

                    let (standard, version): (String, String) = match self {
                        #(#event_meta),*
                    };

                    #[derive(::near_sdk::serde::Serialize)]
                    #[serde(crate="::near_sdk::serde")]
                    #[serde(rename_all="snake_case")]
                    struct EventBuilder #custom_impl_generics #where_clause {
                        standard: String,
                        version: String,
                        #[serde(flatten)]
                        event_data: &#event_lifetime #name #type_generics
                    }
                    let event = EventBuilder { standard, version, event_data: self };
                    let json = ::near_sdk::serde_json::to_string(&event)
                            .unwrap_or_else(|_| ::near_sdk::env::abort());
                    ::near_sdk::env::log_str(&::std::format!("EVENT_JSON:{}", json));
                }
            }
        })
    } else {
        TokenStream::from(
            syn::Error::new(
                Span::call_site(),
                "EventMetadata can only be used as a derive on enums.",
            )
            .to_compile_error(),
        )
    }
}

/// This attribute macro is used on a struct or enum to generate the necessary code for an error
///  returned from a contract method call.
///
/// # Example:
///  
/// For the error:
/// ```ignore
/// #[contract_error]
/// pub struct MyError {
///    field1: String,
///    field2: String,
/// }
/// ```
///
/// And the function:
/// ```ignore
/// pub fn my_method(&self) -> Result<(), MyError>;
/// ```
///
/// The error is serialized as a JSON object with the following structure:
/// ```ignore
/// {
///    "error": {
///       // this name can be "SDK_CONTRACT_ERROR" and "CUSTOM_CONTRACT_ERROR"
///       // To generate "SDK_CONTRACT_ERROR", use sdk attribute `#[contract_error(sdk)]`.
///       // Otherwise, it will generate "CUSTOM_CONTRACT_ERROR"
///       "name": "CUSTOM_CONTRACT_ERROR",
///       "cause": {
///         // this name is the name of error struct
///         "name": "MyError",
///         "info": {
///             /// fields of the error struct
///             "field1": "value1",
///             "field2": "value2"
///         }
///    }
/// }
/// ```
///
/// Note: you can assign any error defined like that to BaseError:
/// ```ignore
/// let base_error: BaseError = MyError { field1: "value1".to_string(), field2: "value2".to_string() }.into();
/// ```
///
/// Use inside_nearsdk attribute (#[contract_error(inside_nearsdk)]) if the error struct is defined inside near-sdk.
/// Don't use if it is defined outside.
///
/// Internally, it makes error struct to:
///  - implement `near_sdk::ContractErrorTrait` so that it becomes correct error
/// which can be returned from contract method with defined structure.
///  - implement `From<ErrorStruct> for near_sdk::BaseError` as a polymorphic solution
///  - implement `From<ErrorStruct> for String` to convert the error to a string

#[derive(FromMeta)]
struct ContractErrorArgs {
    sdk: Option<bool>,
    inside_nearsdk: Option<bool>,
}
#[proc_macro_attribute]
pub fn contract_error(attr: TokenStream, item: TokenStream) -> TokenStream {
    let meta_list = match NestedMeta::parse_meta_list(attr.into()) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(Error::from(e).write_errors());
        }
    };

    let contract_error_args = match ContractErrorArgs::from_list(&meta_list) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };

    let input = syn::parse_macro_input!(item as syn::DeriveInput);
    let ident = &input.ident;

    let error_type = if contract_error_args.sdk.unwrap_or(false) {
        quote! {"SDK_CONTRACT_ERROR"}
    } else {
        quote! {"CUSTOM_CONTRACT_ERROR"}
    };

    let near_sdk_crate: proc_macro2::TokenStream;
    let mut bool_inside_nearsdk_for_macro = quote! {false};

    if contract_error_args.inside_nearsdk.unwrap_or(false) {
        near_sdk_crate = quote! {crate};
        bool_inside_nearsdk_for_macro = quote! {true};
    } else {
        near_sdk_crate = quote! {::near_sdk};
    };

    let mut expanded = quote! {
        #[#near_sdk_crate ::near(serializers=[json], inside_nearsdk=#bool_inside_nearsdk_for_macro)]
        #input

        impl #near_sdk_crate ::ContractErrorTrait for #ident {
            fn error_type(&self) -> &'static str {
                #error_type
            }

            fn wrap(&self) -> #near_sdk_crate ::serde_json::Value {
                #[#near_sdk_crate ::near(inside_nearsdk=#bool_inside_nearsdk_for_macro, serializers = [json])]
                struct ErrorWrapper<T> {
                    name: String,
                    cause: ErrorCause<T>,
                }

                #[#near_sdk_crate ::near(inside_nearsdk=#bool_inside_nearsdk_for_macro, serializers = [json])]
                struct ErrorCause<T> {
                    name: String,
                    info: T
                }

                #near_sdk_crate ::serde_json::json! {
                    { "error" : ErrorWrapper {
                        name: String::from(self.error_type()),
                        cause: ErrorCause {
                            name: std::any::type_name::<#ident>().to_string(),
                            info: self
                        }
                    } }
                }
            }
        }
    };
    if *ident != "BaseError" {
        expanded.extend(quote! {
            impl From<#ident> for String {
                fn from(err: #ident) -> Self {
                    #near_sdk_crate ::serde_json::json!{#near_sdk_crate ::wrap_error(err)}.to_string()
                }
            }

            impl From<#ident> for #near_sdk_crate ::BaseError {
                fn from(err: #ident) -> Self {
                    #near_sdk_crate ::BaseError{
                        error: #near_sdk_crate ::wrap_error(err),
                    }
                }
            }
        });
    }
    TokenStream::from(expanded)
}
