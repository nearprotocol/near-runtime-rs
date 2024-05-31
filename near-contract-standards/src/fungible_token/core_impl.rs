use crate::fungible_token::core::FungibleTokenCore;
use crate::fungible_token::events::{FtBurn, FtTransfer};
use crate::fungible_token::receiver::ext_ft_receiver;
use crate::fungible_token::resolver::{ext_ft_resolver, FungibleTokenResolver};
use near_sdk::collections::LookupMap;
use near_sdk::errors::{
    InsufficientBalance, InsufficientGas, InvalidArgument, TotalSupplyOverflow,
};
use near_sdk::json_types::U128;
use near_sdk::{
    assert_one_yocto, contract_error, env, log, near, require, unwrap_or_err, AccountId, Gas,
    IntoStorageKey, PromiseOrValue, PromiseResult, StorageUsage,
};
use near_sdk::{require_or_err, BaseError};

const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas::from_tgas(5);
const GAS_FOR_FT_TRANSFER_CALL: Gas = Gas::from_tgas(30);

pub type Balance = u128;

/// Implementation of a FungibleToken standard.
/// Allows to include NEP-141 compatible token to any contract.
/// There are next traits that any contract may implement:
///     - FungibleTokenCore -- interface with ft_transfer methods. FungibleToken provides methods for it.
///     - FungibleTokenMetaData -- return metadata for the token in NEP-148, up to contract to implement.
///     - StorageManager -- interface for NEP-145 for allocating storage per account. FungibleToken provides methods for it.
///     - AccountRegistrar -- interface for an account to register and unregister
///
/// For example usage, see examples/fungible-token/src/lib.rs.
#[near]
pub struct FungibleToken {
    /// AccountID -> Account balance.
    pub accounts: LookupMap<AccountId, Balance>,

    /// Total supply of the all token.
    pub total_supply: Balance,

    /// The storage size in bytes for one account.
    pub account_storage_usage: StorageUsage,
}

impl FungibleToken {
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        let mut this =
            Self { accounts: LookupMap::new(prefix), total_supply: 0, account_storage_usage: 0 };
        this.measure_account_storage_usage();
        this
    }

    fn measure_account_storage_usage(&mut self) {
        let initial_storage_usage = env::storage_usage();
        let tmp_account_id = "a".repeat(64).parse().unwrap();
        self.accounts.insert(&tmp_account_id, &0u128);
        self.account_storage_usage = env::storage_usage() - initial_storage_usage;
        self.accounts.remove(&tmp_account_id);
    }

    pub fn internal_unwrap_balance_of(
        &self,
        account_id: &AccountId,
    ) -> Result<Balance, AccountNotRegistered> {
        match self.accounts.get(account_id) {
            Some(balance) => Ok(balance),
            None => Err(AccountNotRegistered::new(account_id.clone())),
        }
    }

    pub fn internal_deposit(
        &mut self,
        account_id: &AccountId,
        amount: Balance,
    ) -> Result<(), BaseError> {
        let balance_result = self.internal_unwrap_balance_of(account_id);
        let balance: u128;
        if let Ok(unwrapped_balance) = balance_result {
            balance = unwrapped_balance;
        } else {
            return Err(balance_result.unwrap_err().into());
        }
        if let Some(new_balance) = balance.checked_add(amount) {
            self.accounts.insert(account_id, &new_balance);
            let checked = self.total_supply.checked_add(amount);
            match checked {
                Some(new_total_supply) => {
                    self.total_supply = new_total_supply;
                    Ok(())
                }
                None => Err(TotalSupplyOverflow {}.into()),
            }
        } else {
            Err(BalanceOverflow {}.into())
        }
    }

    pub fn internal_withdraw(
        &mut self,
        account_id: &AccountId,
        amount: Balance,
    ) -> Result<(), BaseError> {
        let balance_result = self.internal_unwrap_balance_of(account_id);
        let balance: u128;
        if let Ok(unwrapped_balance) = balance_result {
            balance = unwrapped_balance;
        } else {
            return Err(balance_result.unwrap_err().into());
        }
        if let Some(new_balance) = balance.checked_sub(amount) {
            self.accounts.insert(account_id, &new_balance);
            let checked = self.total_supply.checked_sub(amount);
            match checked {
                Some(new_total_supply) => {
                    self.total_supply = new_total_supply;
                    Ok(())
                }
                None => Err(TotalSupplyOverflow {}.into()),
            }
        } else {
            Err(InsufficientBalance::new(None).into())
        }
    }

    pub fn internal_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        amount: Balance,
        memo: Option<String>,
    ) -> Result<(), BaseError> {
        require_or_err!(sender_id != receiver_id, ReceiverIsSender::new());
        require_or_err!(amount > 0, InvalidArgument::new("The amount should be a positive number"));
        unwrap_or_err!(self.internal_withdraw(sender_id, amount));
        unwrap_or_err!(self.internal_deposit(receiver_id, amount));
        FtTransfer {
            old_owner_id: sender_id,
            new_owner_id: receiver_id,
            amount: U128(amount),
            memo: memo.as_deref(),
        }
        .emit();
        Ok(())
    }

    pub fn internal_register_account(
        &mut self,
        account_id: &AccountId,
    ) -> Result<(), AccountAlreadyRegistered> {
        if self.accounts.insert(account_id, &0).is_some() {
            return Err(AccountAlreadyRegistered {});
        }
        Ok(())
    }
}

#[contract_error]
pub struct ReceiverIsSender {
    pub message: String,
}

impl ReceiverIsSender {
    pub fn new() -> Self {
        Self { message: "The receiver should be different from the sender".to_string() }
    }
}

impl Default for ReceiverIsSender {
    fn default() -> Self {
        Self::new()
    }
}

impl FungibleTokenCore for FungibleToken {
    fn ft_transfer(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
    ) -> Result<(), BaseError> {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        let amount: Balance = amount.into();
        self.internal_transfer(&sender_id, &receiver_id, amount, memo)
    }

    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert_one_yocto();
        require!(env::prepaid_gas() > GAS_FOR_FT_TRANSFER_CALL, &String::from(InsufficientGas {}));
        let sender_id = env::predecessor_account_id();
        let amount: Balance = amount.into();
        self.internal_transfer(&sender_id, &receiver_id, amount, memo)
            .unwrap_or_else(|err| env::panic_err(err));
        let receiver_gas = env::prepaid_gas()
            .checked_sub(GAS_FOR_FT_TRANSFER_CALL)
            .unwrap_or_else(|| env::panic_err(InsufficientGas {}.into()));
        // Initiating receiver's call and the callback
        ext_ft_receiver::ext(receiver_id.clone())
            .with_static_gas(receiver_gas)
            .ft_on_transfer(sender_id.clone(), amount.into(), msg)
            .then(
                ext_ft_resolver::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                    .ft_resolve_transfer(sender_id, receiver_id, amount.into()),
            )
            .into()
    }

    fn ft_total_supply(&self) -> U128 {
        self.total_supply.into()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.accounts.get(&account_id).unwrap_or(0).into()
    }
}

impl FungibleToken {
    /// Internal method that returns the amount of burned tokens in a corner case when the sender
    /// has deleted (unregistered) their account while the `ft_transfer_call` was still in flight.
    /// Returns (Used token amount, Burned token amount)
    pub fn internal_ft_resolve_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> Result<(u128, u128), BaseError> {
        let amount: Balance = amount.into();

        // Get the unused amount from the `ft_on_transfer` call result.
        let unused_amount = match env::promise_result(0) {
            PromiseResult::Successful(value) => {
                if let Ok(unused_amount) = near_sdk::serde_json::from_slice::<U128>(&value) {
                    std::cmp::min(amount, unused_amount.0)
                } else {
                    amount
                }
            }
            PromiseResult::Failed => amount,
        };

        if unused_amount > 0 {
            let receiver_balance = self.accounts.get(&receiver_id).unwrap_or(0);
            if receiver_balance > 0 {
                let refund_amount = std::cmp::min(receiver_balance, unused_amount);
                if let Some(new_receiver_balance) = receiver_balance.checked_sub(refund_amount) {
                    self.accounts.insert(&receiver_id, &new_receiver_balance);
                } else {
                    return Err(InsufficientBalance::new(Some(
                        "The receiver account doesn't have enough balance",
                    ))
                    .into());
                }

                if let Some(sender_balance) = self.accounts.get(sender_id) {
                    if let Some(new_sender_balance) = sender_balance.checked_add(refund_amount) {
                        self.accounts.insert(sender_id, &new_sender_balance);
                    } else {
                        return Err(InsufficientBalance::new(None).into());
                    }

                    FtTransfer {
                        old_owner_id: &receiver_id,
                        new_owner_id: sender_id,
                        amount: U128(refund_amount),
                        memo: Some("refund"),
                    }
                    .emit();
                    let used_amount = amount.checked_sub(refund_amount);
                    let Some(used_amount) = used_amount else {
                        return Err(TotalSupplyOverflow {}.into());
                    };
                    return Ok((used_amount, 0));
                } else {
                    // Sender's account was deleted, so we need to burn tokens.
                    let checked = self.total_supply.checked_sub(refund_amount);
                    match checked {
                        Some(new_total_supply) => {
                            self.total_supply = new_total_supply;
                            log!("The account of the sender was deleted");
                            FtBurn {
                                owner_id: &receiver_id,
                                amount: U128(refund_amount),
                                memo: Some("refund"),
                            }
                            .emit();
                            return Ok((amount, refund_amount));
                        }
                        None => {
                            return Err(TotalSupplyOverflow {}.into());
                        }
                    }
                }
            }
        }
        Ok((amount, 0))
    }
}

impl FungibleTokenResolver for FungibleToken {
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> Result<U128, BaseError> {
        let transfer = self.internal_ft_resolve_transfer(&sender_id, receiver_id, amount);
        match transfer {
            Ok((used_amount, _)) => Ok(used_amount.into()),
            Err(err) => Err(err),
        }
    }
}

#[contract_error]
pub struct AccountNotRegistered {
    account_id: AccountId,
}

impl AccountNotRegistered {
    pub fn new(account_id: AccountId) -> Self {
        Self { account_id }
    }
}

#[contract_error]
pub struct AccountAlreadyRegistered {}

#[contract_error]
pub struct BalanceOverflow {}
