//! Helper methods that often used in smart contracts.

pub(crate) mod storage_key_impl;

mod stable_map;
pub(crate) use self::stable_map::StableMap;
mod cache_entry;
pub(crate) use cache_entry::{CacheEntry, EntryState};
mod contract_error;
pub use contract_error::{check_contract_error_trait, wrap_error, BaseError, ContractErrorTrait};

use crate::{env, NearToken, PromiseResult};

/// Helper macro to log a message through [`env::log_str`].
/// This macro can be used similar to the [`std::format`] macro in most cases.
///
/// This differs from [`std::format`] because instead of generating a string, it will log the utf8
/// bytes as a log through [`env::log_str`].
///
/// The logged message will get persisted on chain.
///
/// # Example use
///
/// ```no_run
/// use near_sdk::log;
///
/// # fn main() {
/// log!("test");
/// let world: &str = "world";
/// log!(world);
/// log!("Hello {}", world);
/// log!("x = {}, y = {y}", 10, y = 30);
/// # }
/// ```
///
/// [`env::log_str`]: crate::env::log_str
#[macro_export]
macro_rules! log {
    ($arg:expr) => {
        $crate::env::log_str($arg.as_ref())
    };
    ($($arg:tt)*) => {
        $crate::env::log_str(format!($($arg)*).as_str())
    };
}

/// Helper macro to create assertions that will panic through the runtime host functions.
///
/// This macro can be used similarly to [`assert!`] but will reduce code size by not including
/// file and rust specific data in the panic message.
///
/// Panics with near_sdk::errors::RequireFailed unless error message provided
///
/// # Examples
///
/// ```no_run
/// use near_sdk::require;
///
/// # fn main() {
/// let a = 2;
/// require!(a > 0);
/// require!("test" != "other", "Some custom error message if false");
/// # }
/// ```
#[macro_export]
macro_rules! require {
    ($cond:expr $(,)?) => {
        if cfg!(debug_assertions) {
            assert!($cond)
        } else if !$cond {
            $crate::env::panic_err(::near_sdk::errors::RequireFailed::new().into());
        }
    };
    ($cond:expr, $message:expr $(,)?) => {
        if cfg!(debug_assertions) {
            // Error message must be &str to match panic_str signature
            let msg: &str = &$message;
            assert!($cond, "{}", msg)
        } else if !$cond {
            $crate::env::panic_str(&$message)
        }
    };
}

/// Helper macro to create assertions that will return an error.
///
/// This macro can be used similarly to [`require!`] but will return an error instead of panicking.
///
/// Returns Err(near_sdk::errors::RequireFailed) unless error message provided
///
/// # Examples
///
/// ```no_run
/// use near_sdk::require_or_err;
/// use near_sdk::BaseError;
/// use near_sdk::errors::ContractError;
///
/// # fn f() -> Result<(), BaseError> {
/// let a = 2;
/// require_or_err!(a > 0);
/// require_or_err!("test" != "other", ContractError::new("Some custom error message if false"));
/// Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! require_or_err {
    ($cond:expr $(,)?) => {
        if !$cond {
            return Err(::near_sdk::errors::RequireFailed::new().into());
        }
    };
    ($cond:expr, $err:expr $(,)?) => {
        if !$cond {
            return Err($err.into());
        }
    };
}

/// Helper macro to unwrap an Option or Result, returning an error if None or Err.
///
///  - If you have an option you would like to unwrap, you use unwrap_or_err! on it and
/// provide an error that will be returned from the function in case the option value is None
///
///  - If you have a result you would like to unwrap, you use unwrap_or_err! on it and
/// the error will be returned from the function in case the result is an Err
///
/// # Examples
///
/// ```no_run
/// use near_sdk::unwrap_or_err;
/// use near_sdk::errors::ContractError;
///
/// # fn method() -> Result<u64, ContractError> {
///
/// let option_some: Option<u64> = Some(5);
/// let option_none: Option<u64> = None;
///
/// let result_ok: Result<u64, ContractError> = Ok(5);
/// let result_err: Result<u64, ContractError> = Err(ContractError::new("Some error"));
///
/// let option_success: u64 = unwrap_or_err!(option_some, ContractError::new("Some error")); // option_success == 5
/// let option_error: u64 = unwrap_or_err!(option_none, ContractError::new("Some error")); // error is returned from main
///
/// let result_success: u64 = unwrap_or_err!(result_ok); // result_success == 5
/// let result_error: u64 = unwrap_or_err!(result_err); // error is returned from main
///
/// Ok(0)
/// # }
///```
#[macro_export]
macro_rules! unwrap_or_err {
    ( $exp:expr, $err:expr ) => {
        match $exp {
            Some(x) => x,
            None => {
                return Err($err.into());
            }
        }
    };
    ( $exp:expr ) => {
        match $exp {
            Ok(x) => x,
            Err(err) => {
                return Err(err.into());
            }
        }
    };
}

/// Assert that predecessor_account_id == current_account_id, meaning contract called itself.
pub fn assert_self() {
    require!(env::predecessor_account_id() == env::current_account_id(), "Method is private");
}

/// Assert that 1 yoctoNEAR was attached.
pub fn assert_one_yocto() {
    require!(
        env::attached_deposit() == NearToken::from_yoctonear(1),
        "Requires attached deposit of exactly 1 yoctoNEAR"
    )
}

/// Returns true if promise was successful.
/// Fails if called outside a callback that received 1 promise result.
pub fn is_promise_success() -> bool {
    require!(env::promise_results_count() == 1, "Contract expected a result on the callback");
    env::promise_result_internal(0).is_ok()
}

/// Returns the result of the promise if successful. Otherwise returns None.
/// Fails if called outside a callback that received 1 promise result.
pub fn promise_result_as_success() -> Option<Vec<u8>> {
    require!(env::promise_results_count() == 1, "Contract expected a result on the callback");
    match env::promise_result(0) {
        PromiseResult::Successful(result) => Some(result),
        _ => None,
    }
}

/// Deprecated helper function which used to generate code to initialize the [`GlobalAllocator`].
/// This is now initialized by default. Disable `wee_alloc` feature to configure manually.
///
/// [`GlobalAllocator`]: std::alloc::GlobalAlloc
#[deprecated(
    since = "4.0.0",
    note = "Allocator is already initialized with the default `wee_alloc` feature set. \
            Please make sure you don't disable default features on the SDK or set the global \
            allocator manually."
)]
#[macro_export]
macro_rules! setup_alloc {
    () => {};
}

#[cfg(test)]
mod tests {
    use crate::test_utils::get_logs;

    #[test]
    fn test_log_simple() {
        log!("hello");

        assert_eq!(get_logs(), vec!["hello".to_string()]);
    }

    #[test]
    fn test_log_format() {
        log!("hello {} ({})", "user_name", 25);

        assert_eq!(get_logs(), vec!["hello user_name (25)".to_string()]);
    }
}
