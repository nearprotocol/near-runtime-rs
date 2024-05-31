use near_sdk::json_types::U128;
use near_sdk::serde_json;
use near_sdk::{env, near, AccountId, Gas, NearToken, PromiseResult};
use near_sdk::errors::PromiseFailed;
use near_sdk::BaseError;

// Prepaid gas for making a single simple call.
const SINGLE_CALL_GAS: Gas = Gas::from_tgas(20);

#[derive(Default)]
#[near(contract_state)]
pub struct FactoryContract {}

#[near]
impl FactoryContract {
    pub fn deploy_status_message(&self, account_id: AccountId, amount: U128) {
        let promise_idx = env::promise_batch_create(&account_id);
        env::promise_batch_action_create_account(promise_idx);
        env::promise_batch_action_transfer(promise_idx, NearToken::from_yoctonear(amount.0));
        env::promise_batch_action_add_key_with_full_access(
            promise_idx,
            &env::signer_account_pk(),
            0,
        );
        let code: &[u8] = include_bytes!("../../../status-message/res/status_message.wasm");
        env::promise_batch_action_deploy_contract(promise_idx, code);
    }

    pub fn simple_call(&mut self, account_id: AccountId, message: String) {
        env::promise_create(
            account_id,
            "set_status",
            &serde_json::to_vec(&(message,)).unwrap(),
            NearToken::from_near(0),
            SINGLE_CALL_GAS,
        );
    }
    pub fn complex_call(&mut self, account_id: AccountId, message: String) {
        // 1) call status_message to record a message from the signer.
        // 2) check that the promise succeed
        // 3) call status_message to retrieve the message of the signer.
        // 4) return that message as its own result.
        // Note, for a contract to simply call another contract (1) is sufficient.
        let promise0 = env::promise_create(
            account_id.clone(),
            "set_status",
            &serde_json::to_vec(&(message,)).unwrap(),
            NearToken::from_near(0),
            SINGLE_CALL_GAS,
        );
        let promise1 = env::promise_then(
            promise0,
            env::current_account_id(),
            "get_result",
            &serde_json::to_vec(&(account_id,)).unwrap(),
            NearToken::from_near(0),
            SINGLE_CALL_GAS.saturating_mul(2),
        );
        env::promise_return(promise1);
    }

    pub fn get_result(&mut self, account_id: AccountId) -> Result<(), BaseError>{
        match env::promise_result(0) {
            PromiseResult::Successful(_) => {
                env::promise_return(env::promise_create(
                    account_id,
                    "get_status",
                    &serde_json::to_vec(&(env::signer_account_id(),)).unwrap(),
                    NearToken::from_near(0),
                    SINGLE_CALL_GAS,
                ));
                return Ok(());
            }
            _ => {
                return Err(PromiseFailed::new(Some(0), Some("Failed to set status")).into());
            },
        };
    }
}
