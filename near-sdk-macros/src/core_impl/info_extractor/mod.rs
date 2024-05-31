use syn::{Receiver, ReturnType, Type};

mod serializer_attr;
pub use serializer_attr::SerializerAttr;

mod arg_info;
pub use arg_info::{ArgInfo, BindgenArgType};

mod handle_result_attr;
pub use handle_result_attr::HandleResultAttr;

mod attr_sig_info;
pub use attr_sig_info::AttrSigInfo;

mod impl_item_method_info;
pub use impl_item_method_info::ImplItemMethodInfo;

mod trait_item_method_info;
pub use trait_item_method_info::*;

mod item_trait_info;
pub use item_trait_info::ItemTraitInfo;

mod item_impl_info;

mod init_attr;
pub use init_attr::InitAttr;

mod visitor;

pub use item_impl_info::ItemImplInfo;

/// Type of serialization we use.
#[derive(Clone, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum SerializerType {
    JSON,
    Borsh,
}

#[derive(Clone, PartialEq, Eq)]
pub enum MethodKind {
    Call(CallMethod),
    View(ViewMethod),
    Init(InitMethod),
}

#[derive(Clone, PartialEq, Eq)]
pub struct CallMethod {
    /// Whether method accepting $NEAR.
    pub is_payable: bool,
    /// Whether method can accept calls from self (current account)
    pub is_private: bool,
    /// The serializer that we use for the return type.
    pub result_serializer: SerializerType,
    /// The receiver, like `mut self`, `self`, `&mut self`, `&self`, or `None`.
    pub receiver: Option<Receiver>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ViewMethod {
    /// Whether method can accept calls from self (current account)
    pub is_private: bool,
    /// The serializer that we use for the return type.
    pub result_serializer: SerializerType,
    /// The receiver, like `mut self`, `self`, `&mut self`, `&self`, or `None`.
    pub receiver: Option<Receiver>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct InitMethod {
    /// Whether method accepting $NEAR.
    pub is_payable: bool,
    /// Whether init method ignores state
    pub ignores_state: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Returns {
    /// The original return type of the method in the Rust AST.
    pub original: ReturnType,
    /// The return type of the method in our logic.
    pub kind: ReturnKind,
}

#[derive(Clone, PartialEq, Eq)]
pub enum ReturnKind {
    /// Return type is not specified.
    ///
    /// Functions default to `()` and closures default to type inference.
    /// When the contract call happens:
    ///  - Contract struct is initialized
    ///  - The method is called
    ///  - Contract state is written if it is modifying method.
    /// In case of panic, state is not written.
    ///
    /// # Example:
    /// ```ignore
    /// pub fn foo(&mut self);
    /// ```
    Default,

    /// Return type is specified. But it does not have any specifics.
    ///
    /// When the contract call happens, in addition to the Default:
    ///  - The return value is serialized and returned
    ///
    /// # Example:
    /// ```ignore
    /// pub fn foo(&mut self) -> u64;
    /// ```
    General(Type),

    /// Return type is Result<OkType, ErrType> and the function is marked with #[handle_result].
    /// ErrType struct implements near_sdk::FunctionError. (i.e. used with #[derive(near_sdk::FunctionError)])
    ///
    /// When the contract call happens, in addition to the General:
    ///  - In case Result value is Ok, the unwrapped object is returned
    ///  - In case Result value is Err, panic is called and state is not written.
    ///
    /// # Example:
    /// ```ignore
    /// #[handle_result]
    /// pub fn foo(&mut self) -> Result<u64, &'static str>;
    /// ```
    HandlesResultExplicit(Type),

    /// Return type is Result<OkType, ErrType> and, the function is not marked with #[handle_result] and
    /// ErrType struct implements near_sdk::ContractErrorTrait (i.e. used with #[near_sdk::contract_error])
    ///
    /// When the contract call happens, in addition to General:
    ///  - In case Result value is Err, panic is called and state is not written.
    /// As soon as ErrType implements ContractErrorTrait, it is returned as a well-defined structure.
    /// You can see the structure in #[contract_error] documentation.
    /// If the error struct does not implement ContractErrorTrait, the code should not compile.
    ///  - In case #[persist_on_error] is used on method, panic is not called.
    /// And the contract state is written safely.
    /// But the extra <method_name>_error method is generated.
    /// And this method is called in a new Promise.
    /// This method effectively panics with structured error.
    ///
    /// # Example:
    /// ```ignore
    /// #[contract_error]
    /// pub struct MyError;
    ///
    /// // if Ok() is returned, everything ok, otherwise panic with well-structured error
    /// pub fn foo(&mut self) -> Result<u64, MyError>;
    /// ```
    ///
    /// ```ignore
    /// // Write state safely anyway.
    /// // if Ok() is returned, just return. Otherwise call new Promise which will panic with well-structured error.
    /// #[persist_on_error]
    /// pub fn foo(&mut self) -> Result<u64, MyError>;
    /// ```
    ///
    HandlesResultImplicit(StatusResult),
}
/// In other cases the code should not compile

#[derive(Clone, PartialEq, Eq)]
pub struct StatusResult {
    pub result_type: Type,
    pub persist_on_error: bool,
}
