/*!
A stub contract that implements nft_on_transfer for simulation testing nft_transfer_call.
*/
use near_contract_standards::non_fungible_token::core::NonFungibleTokenReceiver;
use near_contract_standards::non_fungible_token::TokenId;
use near_sdk::{env, log, near, require, AccountId, Gas, PanicOnDefault, PromiseOrValue};
use near_sdk::errors::{InvalidArgument, PermissionDenied};

/// It is estimated that we need to attach 5 TGas for the code execution and 5 TGas for cross-contract call
const GAS_FOR_NFT_ON_TRANSFER: Gas = Gas::from_tgas(10);

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct TokenReceiver {
    non_fungible_token_account_id: AccountId,
}

// Have to repeat the same trait for our own implementation.
trait ValueReturnTrait {
    fn ok_go(&self, return_it: bool) -> PromiseOrValue<bool>;
}

#[near]
impl TokenReceiver {
    #[init]
    pub fn new(non_fungible_token_account_id: AccountId) -> Self {
        Self { non_fungible_token_account_id }
    }
}

#[near]
impl NonFungibleTokenReceiver for TokenReceiver {
    /// Returns true if token should be returned to `sender_id`
    /// Four supported `msg`s:
    /// * "return-it-now" - immediately return `true`
    /// * "keep-it-now" - immediately return `false`
    /// * "return-it-later" - make cross-contract call which resolves with `true`
    /// * "keep-it-later" - make cross-contract call which resolves with `false`
    /// Otherwise panics, which should also return token to `sender_id`
    fn nft_on_transfer(
        &mut self,
        sender_id: AccountId,
        previous_owner_id: AccountId,
        token_id: TokenId,
        msg: String,
    ) -> PromiseOrValue<bool> {
        // Verifying that we were called by non-fungible token contract that we expect.
        require!(
            env::predecessor_account_id() == self.non_fungible_token_account_id,
            &String::from(PermissionDenied::new(Some("Only supports the one fungible token contract")))
        );
        log!(
            "in nft_on_transfer; sender_id={}, previous_owner_id={}, token_id={}, msg={}",
            &sender_id,
            &previous_owner_id,
            &token_id,
            msg
        );
        match msg.as_str() {
            "return-it-now" => PromiseOrValue::Value(true),
            "return-it-later" => {
                let prepaid_gas = env::prepaid_gas();
                let account_id = env::current_account_id();
                Self::ext(account_id)
                    .with_static_gas(prepaid_gas.saturating_sub(GAS_FOR_NFT_ON_TRANSFER))
                    .ok_go(true)
                    .into()
            }
            "keep-it-now" => PromiseOrValue::Value(false),
            "keep-it-later" => {
                let prepaid_gas = env::prepaid_gas();
                let account_id = env::current_account_id();
                Self::ext(account_id)
                    .with_static_gas(prepaid_gas.saturating_sub(GAS_FOR_NFT_ON_TRANSFER))
                    .ok_go(false)
                    .into()
            }
            _ => env::panic_err(InvalidArgument::new("Unsupported msg")),
        }
    }
}

#[near]
impl ValueReturnTrait for TokenReceiver {
    fn ok_go(&self, return_it: bool) -> PromiseOrValue<bool> {
        log!("in ok_go, return_it={}", return_it);
        PromiseOrValue::Value(return_it)
    }
}
