use near_sdk::errors::InsufficientBalance;
use near_sdk::{contract_error, env, require_or_err, AccountId, BaseError, NearToken, Promise};
use std::collections::HashMap;
use std::mem::size_of;

// TODO: need a way for end users to determine how much an approval will cost.
pub fn bytes_for_approved_account_id(account_id: &AccountId) -> u64 {
    // The extra 4 bytes are coming from Borsh serialization to store the length of the string.
    account_id.as_str().len() as u64 + 4 + size_of::<u64>() as u64
}

pub fn refund_approved_account_ids_iter<'a, I>(
    account_id: AccountId,
    approved_account_ids: I,
) -> Promise
where
    I: Iterator<Item = &'a AccountId>,
{
    let storage_released: u64 = approved_account_ids.map(bytes_for_approved_account_id).sum();
    Promise::new(account_id)
        .transfer(env::storage_byte_cost().saturating_mul(storage_released.into()))
}

pub fn refund_approved_account_ids(
    account_id: AccountId,
    approved_account_ids: &HashMap<AccountId, u64>,
) -> Promise {
    refund_approved_account_ids_iter(account_id, approved_account_ids.keys())
}

pub fn refund_deposit_to_account(
    storage_used: u64,
    account_id: AccountId,
) -> Result<(), BaseError> {
    let required_cost = env::storage_byte_cost().saturating_mul(storage_used.into());
    let attached_deposit = env::attached_deposit();

    require_or_err!(
        required_cost <= attached_deposit,
        InsufficientBalance::new(Some(
            format!("Must attach {} yoctoNEAR to cover storage", required_cost).as_str()
        ))
    );

    let refund = attached_deposit.saturating_sub(required_cost);
    if refund.as_yoctonear() > 1 {
        Promise::new(account_id).transfer(refund);
    }
    Ok(())
}

/// Assumes that the precedecessor will be refunded
pub fn refund_deposit(storage_used: u64) -> Result<(), BaseError> {
    refund_deposit_to_account(storage_used, env::predecessor_account_id())
}

/// Assert that at least 1 yoctoNEAR was attached.
pub(crate) fn assert_at_least_one_yocto() -> Result<(), BaseError> {
    require_or_err!(
        env::attached_deposit() >= NearToken::from_yoctonear(1),
        InsufficientBalance::new(Some("Requires attached deposit of at least 1 yoctoNEAR"))
    );
    Ok(())
}

#[contract_error]
pub struct ApprovalNotSupported {
    message: String,
}

impl ApprovalNotSupported {
    pub fn new(message: &str) -> Self {
        Self { message: String::from(message) }
    }
}
