use near_sdk::require_or_err;
use near_sdk::{env, near, BaseError, Promise, PromiseError};
use near_sdk::errors::{InvalidArgument, UnexpectedFailure, InvalidPromiseReturn};

const A_VALUE: u8 = 8;

#[near(contract_state)]
pub struct Callback;

#[near]
impl Callback {
    /// Call functions a, b, and c asynchronously and handle results with `handle_callbacks`.
    pub fn call_all(fail_b: bool, c_value: u8, d_value: u8) -> Promise {
        Self::ext(env::current_account_id())
            .a()
            .and(Self::ext(env::current_account_id()).b(fail_b))
            .and(Self::ext(env::current_account_id()).c(c_value))
            .and(Self::ext(env::current_account_id()).d(d_value))
            .then(Self::ext(env::current_account_id()).handle_callbacks())
    }

    /// Calls function c with a value that will always succeed
    pub fn a() -> Promise {
        Self::ext(env::current_account_id()).c(A_VALUE)
    }

    /// Returns a static string if fail is false, return
    #[private]
    pub fn b(fail: bool) -> Result<&'static str, BaseError> {
        if fail {
            return Err(UnexpectedFailure {
                message: "Failed within function b".to_string(),
            }
            .into());
        }
        Ok("Some string")
    }

    /// Panics if value is 0, returns the value passed in otherwise.
    #[private]
    pub fn c(value: u8) -> Result<u8, InvalidArgument> {
        require_or_err!(value > 0, InvalidArgument::new("Value must be positive"));
        Ok(value)
    }

    /// Panics if value is 0.
    #[private]
    pub fn d(value: u8) -> Result<(), InvalidArgument> {
        require_or_err!(value > 0, InvalidArgument::new("Value must be positive"));
        Ok(())
    }

    /// Receives the callbacks from the other promises called.
    #[private]
    pub fn handle_callbacks(
        #[callback_unwrap] a: u8,
        #[callback_result] b: Result<String, PromiseError>,
        #[callback_result] c: Result<u8, PromiseError>,
        #[callback_result] d: Result<(), PromiseError>,
    ) -> Result<(bool, bool, bool), BaseError> {
        require_or_err!(a == A_VALUE, InvalidPromiseReturn::new("Promise returned incorrect value"));
        if let Ok(s) = b.as_ref() {
            require_or_err!(s == "Some string");
        }
        Ok((b.is_err(), c.is_err(), d.is_err()))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use tokio::fs;

    #[tokio::test]
    async fn workspaces_test() -> anyhow::Result<()> {
        let wasm = fs::read("res/callback_results.wasm").await?;
        let worker = near_workspaces::sandbox().await?;
        let contract = worker.dev_deploy(&wasm).await?;

        // Call function a only to ensure it has correct behaviour
        let res = contract.call("a").transact().await?;
        assert_eq!(res.json::<u8>()?, 8);

        // Following tests the function call where the `call_all` function always succeeds and handles
        // the result of the async calls made from within the function with callbacks.

        // No failures
        let res =
            contract.call("call_all").args_json((false, 1u8, 1u8)).max_gas().transact().await?;
        assert_eq!(res.json::<(bool, bool, bool)>()?, (false, false, false));

        // Fail b
        let res =
            contract.call("call_all").args_json((true, 1u8, 1u8)).max_gas().transact().await?;
        assert_eq!(res.json::<(bool, bool, bool)>()?, (true, false, false));

        // Fail c
        let res =
            contract.call("call_all").args_json((false, 0u8, 1u8)).max_gas().transact().await?;
        assert_eq!(res.json::<(bool, bool, bool)>()?, (false, true, false));

        // Fail d
        let res = contract
            .call("call_all")
            .args_json((false, 1u8, 0u8))
            .gas(near_sdk::Gas::from_tgas(300))
            .transact()
            .await?;
        assert_eq!(res.json::<(bool, bool, bool)>()?, (false, false, true));

        // Fail b and c
        let res = contract
            .call("call_all")
            .args_json((true, 0u8, 1u8))
            .gas(near_sdk::Gas::from_tgas(300))
            .transact()
            .await?;
        assert_eq!(res.json::<(bool, bool, bool)>()?, (true, true, false));

        // Fail all
        let res = contract
            .call("call_all")
            .args_json((true, 0u8, 0u8))
            .gas(near_sdk::Gas::from_tgas(300))
            .transact()
            .await?;
        assert_eq!(res.json::<(bool, bool, bool)>()?, (true, true, true));

        Ok(())
    }
}
