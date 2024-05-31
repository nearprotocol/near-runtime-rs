use crate::fungible_token::{Balance, FungibleToken};
use crate::storage_management::{StorageBalance, StorageBalanceBounds, StorageManagement};
use near_sdk::errors::InsufficientBalance;
use near_sdk::{
    assert_one_yocto, contract_error, env, log, unwrap_or_err, AccountId, BaseError, NearToken,
    Promise,
};

use super::core_impl::AccountNotRegistered;

impl FungibleToken {
    /// Internal method that returns the Account ID and the balance in case the account was
    /// unregistered.
    pub fn internal_storage_unregister(
        &mut self,
        force: Option<bool>,
    ) -> Result<Option<(AccountId, Balance)>, BaseError> {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        let force = force.unwrap_or(false);
        if let Some(balance) = self.accounts.get(&account_id) {
            if balance == 0 || force {
                self.accounts.remove(&account_id);
                self.total_supply -= balance;
                Promise::new(account_id.clone()).transfer(
                    self.storage_balance_bounds().min.saturating_add(NearToken::from_yoctonear(1)),
                );
                Ok(Some((account_id, balance)))
            } else {
                Err(PositiveBalanceUnregistering::new().into())
            }
        } else {
            log!("The account {} is not registered", &account_id);
            Ok(None)
        }
    }

    fn internal_storage_balance_of(&self, account_id: &AccountId) -> Option<StorageBalance> {
        if self.accounts.contains_key(account_id) {
            Some(StorageBalance {
                total: self.storage_balance_bounds().min,
                available: NearToken::from_near(0),
            })
        } else {
            None
        }
    }
}

#[contract_error]
pub struct PositiveBalanceUnregistering {
    pub message: String,
}

impl PositiveBalanceUnregistering {
    pub fn new() -> Self {
        Self {
            message: "Can't unregister the account with the positive balance without force"
                .to_string(),
        }
    }
}

impl Default for PositiveBalanceUnregistering {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageManagement for FungibleToken {
    // `registration_only` doesn't affect the implementation for vanilla fungible token.
    #[allow(unused_variables)]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> Result<StorageBalance, BaseError> {
        let amount = env::attached_deposit();
        let account_id = account_id.unwrap_or_else(env::predecessor_account_id);
        if self.accounts.contains_key(&account_id) {
            log!("The account is already registered, refunding the deposit");
            if amount > NearToken::from_near(0) {
                Promise::new(env::predecessor_account_id()).transfer(amount);
            }
        } else {
            let min_balance = self.storage_balance_bounds().min;
            if amount < min_balance {
                return Err(InsufficientBalance::new(Some(
                    "The attached deposit is less than the minimum storage balance",
                ))
                .into());
            }

            unwrap_or_err!(self.internal_register_account(&account_id));
            let refund = amount.saturating_sub(min_balance);
            if refund > NearToken::from_near(0) {
                Promise::new(env::predecessor_account_id()).transfer(refund);
            }
        }
        Ok(self.internal_storage_balance_of(&account_id).unwrap())
    }

    /// While storage_withdraw normally allows the caller to retrieve `available` balance, the basic
    /// Fungible Token implementation sets storage_balance_bounds.min == storage_balance_bounds.max,
    /// which means available balance will always be 0. So this implementation:
    /// * panics if `amount > 0`
    /// * never transfers Ⓝ to caller
    /// * returns a `storage_balance` struct if `amount` is 0
    fn storage_withdraw(&mut self, amount: Option<NearToken>) -> Result<StorageBalance, BaseError> {
        assert_one_yocto();
        let predecessor_account_id = env::predecessor_account_id();
        if let Some(storage_balance) = self.internal_storage_balance_of(&predecessor_account_id) {
            match amount {
                Some(amount) if amount > NearToken::from_near(0) => Err(InsufficientBalance::new(
                    Some("The amount is greater than the available storage balance"),
                )
                .into()),
                _ => Ok(storage_balance),
            }
        } else {
            Err(AccountNotRegistered::new(predecessor_account_id).into())
        }
    }

    fn storage_unregister(&mut self, force: Option<bool>) -> Result<bool, BaseError> {
        match self.internal_storage_unregister(force) {
            Ok(unregistered) => Ok(unregistered.is_some()),
            Err(err) => Err(err),
        }
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        let required_storage_balance =
            env::storage_byte_cost().saturating_mul(self.account_storage_usage.into());
        StorageBalanceBounds { min: required_storage_balance, max: Some(required_storage_balance) }
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.internal_storage_balance_of(&account_id)
    }
}
