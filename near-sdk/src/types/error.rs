use crate::errors;

/// Enables contract runtime to panic with the given type. Any error type used in conjunction
/// with `#[handle_result]` has to implement this trait.
///
/// ```
/// use near_sdk::FunctionError;
///
/// enum Error {
///     NotFound,
///     Unexpected { message: String },
/// }
///
/// impl FunctionError for Error {
///     fn panic(&self) -> ! {
///         match self {
///             Error::NotFound =>
///                 near_sdk::env::panic_str("not found"),
///             Error::Unexpected { message } =>
///                 near_sdk::env::panic_str(&format!("unexpected error: {}", message))
///         }
///     }
/// }
/// ```
pub trait FunctionError {
    fn panic(&self) -> !;
}

impl<T> FunctionError for T
where
    T: AsRef<str>,
{
    fn panic(&self) -> ! {
        crate::env::panic_err(errors::ContractError::new(self.as_ref()).into())
    }
}

/// A simple type used in conjunction with [FunctionError] representing that the function should
/// abort without a custom message.
///
/// ```
/// use near_sdk::{Abort, near};
///
/// #[near(contract_state)]
/// #[derive(Default)]
/// pub struct Contract;
///
/// #[near]
/// impl Contract {
///     #[handle_result]
///     pub fn foo(&self, text: &str) -> Result<String, Abort> {
///         if text == "success" {
///             Ok("success".to_string())
///         } else {
///             Err(Abort)
///         }
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abort;

impl FunctionError for Abort {
    fn panic(&self) -> ! {
        crate::env::abort()
    }
}
