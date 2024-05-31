/*!
Some hypothetical DeFi contract that will do smart things with the transferred tokens
*/
use near_contract_standards::fungible_token::{receiver::FungibleTokenReceiver, Balance};
use near_sdk::json_types::U128;
use near_sdk::errors::{ContractAlreadyInitialized, PermissionDenied};
use near_sdk::{env, log, near, require, AccountId, Gas, PanicOnDefault, PromiseOrValue};

const BASE_GAS: u64 = 5_000_000_000_000;
const PROMISE_CALL: u64 = 5_000_000_000_000;
const GAS_FOR_FT_ON_TRANSFER: Gas = Gas::from_gas(BASE_GAS + PROMISE_CALL);

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct DeFi {
    fungible_token_account_id: AccountId,
}

// Have to repeat the same trait for our own implementation.
#[allow(dead_code)]
trait ValueReturnTrait {
    fn value_please(&self, amount_to_return: String) -> PromiseOrValue<U128>;
}

#[near]
impl DeFi {
    #[init]
    pub fn new(fungible_token_account_id: AccountId) -> Self {
        require!(!env::state_exists(), &String::from(ContractAlreadyInitialized {}));
        Self { fungible_token_account_id: fungible_token_account_id.into() }
    }
}

#[near]
impl FungibleTokenReceiver for DeFi {
    /// If given `msg: "take-my-money", immediately returns U128::From(0)
    /// Otherwise, makes a cross-contract call to own `value_please` function, passing `msg`
    /// value_please will attempt to parse `msg` as an integer and return a U128 version of it
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        // Verifying that we were called by fungible token contract that we expect.
        require!(
            env::predecessor_account_id() == self.fungible_token_account_id,
            &String::from(PermissionDenied::new(Some(
                "Only supports the one fungible token contract"
            )))
        );
        log!("in {} tokens from @{} ft_on_transfer, msg = {}", amount.0, sender_id, msg);
        match msg.as_str() {
            "take-my-money" => PromiseOrValue::Value(U128::from(0)),
            _ => {
                let prepaid_gas = env::prepaid_gas();
                let account_id = env::current_account_id();
                Self::ext(account_id)
                    .with_static_gas(prepaid_gas.saturating_sub(GAS_FOR_FT_ON_TRANSFER))
                    .value_please(msg)
                    .into()
            }
        }
    }
}

#[near]
impl ValueReturnTrait for DeFi {
    fn value_please(&self, amount_to_return: String) -> PromiseOrValue<U128> {
        log!("in value_please, amount_to_return = {}", amount_to_return);
        let amount: Balance = amount_to_return.parse().expect("Not an integer");
        PromiseOrValue::Value(amount.into())
    }
}
