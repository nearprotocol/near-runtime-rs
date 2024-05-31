use super::{HandleResultAttr, InitAttr, MethodKind, ReturnKind, SerializerAttr};
use crate::core_impl::{utils, CallMethod, InitMethod, Returns, SerializerType, ViewMethod};
use quote::ToTokens;
use syn::{spanned::Spanned, Attribute, Error, FnArg, Receiver, ReturnType, Signature, Type};

/// Traversal abstraction to walk a method declaration and build it's respective [MethodKind].
pub struct Visitor {
    kind: VisitorKind,
    return_type: ReturnType,
    parsed_data: ParsedData,
}

struct ParsedData {
    handles_result: ResultHandling,
    is_payable: bool,
    is_private: bool,
    persist_on_error: bool,
    ignores_state: bool,
    result_serializer: SerializerType,
    receiver: Option<Receiver>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum ResultHandling {
    // No result handling.
    None,
    // Attempt to handle the `Result` without performing a heuristic type check.
    NoCheck,
    // Attempt to handle the `Result` with a heuristic type check.
    Check,
}

impl Default for ResultHandling {
    fn default() -> Self {
        Self::None
    }
}

impl Default for ParsedData {
    fn default() -> Self {
        Self {
            handles_result: Default::default(),
            is_payable: Default::default(),
            is_private: Default::default(),
            persist_on_error: Default::default(),
            ignores_state: Default::default(),
            result_serializer: SerializerType::JSON,
            receiver: Default::default(),
        }
    }
}

#[derive(Debug, strum_macros::Display)]
enum VisitorKind {
    Call,
    View,
    Init,
}

impl Visitor {
    pub fn new(original_attrs: &[Attribute], original_sig: &Signature) -> Self {
        use VisitorKind::*;

        // Run early checks to determine the method type
        let kind = if is_init(original_attrs) {
            Init
        } else if is_view(original_sig) {
            View
        } else {
            Call
        };

        Self { kind, return_type: original_sig.output.clone(), parsed_data: Default::default() }
    }

    pub fn visit_init_attr(&mut self, attr: &Attribute, init_attr: &InitAttr) -> syn::Result<()> {
        use VisitorKind::*;

        match self.kind {
            Init => {
                self.parsed_data.ignores_state = init_attr.ignore_state;
                Ok(())
            }
            Call | View => {
                let message =
                    format!("{} function can't be an init function at the same time.", self.kind);
                Err(Error::new(attr.span(), message))
            }
        }
    }

    pub fn visit_payable_attr(&mut self, attr: &Attribute) -> syn::Result<()> {
        use VisitorKind::*;

        match self.kind {
            Call | Init => {
                self.parsed_data.is_payable = true;
                Ok(())
            }
            View => {
                let message = format!("{} function can't be payable.", self.kind);
                Err(Error::new(attr.span(), message))
            }
        }
    }

    pub fn visit_private_attr(&mut self, _attr: &Attribute) -> syn::Result<()> {
        self.parsed_data.is_private = true;
        Ok(())
    }

    pub fn visit_result_serializer_attr(
        &mut self,
        attr: &Attribute,
        result_serializer_attr: &SerializerAttr,
    ) -> syn::Result<()> {
        use VisitorKind::*;

        match self.kind {
            Call | View => {
                self.parsed_data.result_serializer = result_serializer_attr.serializer_type.clone();
                Ok(())
            }
            Init => {
                let message = format!("{} function can't serialize return type.", self.kind);
                Err(Error::new(attr.span(), message))
            }
        }
    }

    pub fn visit_handle_result_attr(&mut self, params: &HandleResultAttr) {
        self.parsed_data.handles_result =
            if params.check { ResultHandling::NoCheck } else { ResultHandling::Check }
    }

    pub fn visit_persist_on_error_attr(&mut self, _attr: &Attribute) -> syn::Result<()> {
        self.parsed_data.persist_on_error = true;
        Ok(())
    }

    pub fn visit_receiver(&mut self, receiver: &Receiver) -> syn::Result<()> {
        use VisitorKind::*;

        match self.kind {
            Call | View => {
                self.parsed_data.receiver = Some(receiver.clone());
                Ok(())
            }
            Init => {
                let message = format!("{} function can't have `self` parameter.", self.kind);
                Err(Error::new(receiver.span(), message))
            }
        }
    }

    /// Extract the return type of the function. Must be called last as it depends on the value of
    /// `handles_result`. This is why it's private and called as part of `build`.
    fn get_return_type(&mut self) -> syn::Result<Returns> {
        use VisitorKind::*;

        match &self.return_type {
            ReturnType::Default => match self.kind {
                Call | View => {
                    Ok(Returns { original: self.return_type.clone(), kind: ReturnKind::Default })
                }
                Init => {
                    let message = format!("{} function must return the contract state.", self.kind);
                    Err(Error::new(self.return_type.span(), message))
                }
            },
            ReturnType::Type(_, typ) => Ok(Returns {
                original: self.return_type.clone(),
                kind: parse_return_kind(
                    typ,
                    self.parsed_data.handles_result,
                    self.parsed_data.persist_on_error,
                )?,
            }),
        }
    }

    /// Get the method data and the return type from the visited attributes and arguments.
    pub fn build(mut self) -> syn::Result<(MethodKind, Returns)> {
        use VisitorKind::*;

        // Must be called last which is why it's called here.
        let returns = self.get_return_type()?;

        let Visitor { kind, parsed_data, .. } = self;

        let ParsedData {
            is_payable, is_private, ignores_state, result_serializer, receiver, ..
        } = parsed_data;

        let method_kind = match kind {
            Call => {
                MethodKind::Call(CallMethod { is_payable, is_private, result_serializer, receiver })
            }
            Init => MethodKind::Init(InitMethod { is_payable, ignores_state }),
            View => MethodKind::View(ViewMethod { is_private, result_serializer, receiver }),
        };

        Ok((method_kind, returns))
    }
}

fn is_init(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|a| a.path().to_token_stream().to_string() == "init")
}

fn is_view(sig: &Signature) -> bool {
    let receiver_opt = sig.inputs.iter().find_map(|arg| match arg {
        FnArg::Receiver(r) => Some(r),
        _ => None,
    });

    match receiver_opt {
        Some(receiver) => receiver.reference.is_none() || receiver.mutability.is_none(),
        None => true,
    }
}

fn parse_return_kind(
    typ: &Type,
    handles_result: ResultHandling,
    persist_on_error: bool,
) -> syn::Result<ReturnKind> {
    match handles_result {
        ResultHandling::NoCheck => Ok(ReturnKind::HandlesResultExplicit(typ.clone())),
        ResultHandling::Check => {
            if !utils::type_is_result(typ) {
                Err(Error::new(typ.span(), "Function marked with #[handle_result] should return Result<T, E> (where E implements FunctionError). If you're trying to use a type alias for `Result`, try `#[handle_result(aliased)]`."))
            } else {
                Ok(ReturnKind::HandlesResultExplicit(typ.clone()))
            }
        }
        ResultHandling::None => {
            if utils::type_is_result(typ) {
                Ok(ReturnKind::HandlesResultImplicit(crate::StatusResult {
                    result_type: typ.clone(),
                    persist_on_error,
                }))
            } else {
                Ok(ReturnKind::General(typ.clone()))
            }
        }
    }
}
