mod approval_impl;
mod approval_receiver;

pub use approval_receiver::*;

use crate::non_fungible_token::token::TokenId;
use near_sdk::{AccountId, BaseError, Promise};

/// Trait used when it's desired to have a non-fungible token that has a
/// traditional escrow or approval system. This allows Alice to allow Bob
/// to take only the token with the unique identifier "19" but not others.
/// It should be noted that in the [core non-fungible token standard] there
/// is a method to do "transfer and call" which may be preferred over using
/// an approval management standard in certain use cases.
///
/// [approval management standard]: https://nomicon.io/Standards/NonFungibleToken/ApprovalManagement.html
/// [core non-fungible token standard]: https://nomicon.io/Standards/NonFungibleToken/Core.html
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use near_sdk::{PanicOnDefault, AccountId, PromiseOrValue, near, Promise, BaseError};
/// use near_contract_standards::non_fungible_token::{TokenId, NonFungibleToken, NonFungibleTokenApproval};
///
/// #[near(contract_state)]
/// #[derive(PanicOnDefault)]
/// pub struct Contract {
///    tokens: NonFungibleToken,
///}
///
/// #[near]
/// impl NonFungibleTokenApproval for Contract {
///     #[payable]
///     fn nft_approve(&mut self, token_id: TokenId, account_id: AccountId, msg: Option<String>) -> Result<Option<Promise>, BaseError> {
///         self.tokens.nft_approve(token_id, account_id, msg)
///     }
///
///     #[payable]
///     fn nft_revoke(&mut self, token_id: TokenId, account_id: AccountId) -> Result<(), BaseError> {
///         self.tokens.nft_revoke(token_id, account_id)
///     }
///
///     #[payable]
///     fn nft_revoke_all(&mut self, token_id: TokenId) -> Result<(), BaseError> {
///         self.tokens.nft_revoke_all(token_id)
///
///     }
///
///     fn nft_is_approved(&self, token_id: TokenId, approved_account_id: AccountId, approval_id: Option<u64>) -> Result<bool, BaseError> {
///         self.tokens.nft_is_approved(token_id, approved_account_id, approval_id)
///     }
/// }
/// ```
///
pub trait NonFungibleTokenApproval {
    /// Add an approved account for a specific token.
    ///
    /// Requirements
    /// * Caller of the method must attach a deposit of at least 1 yoctoⓃ for
    ///   security purposes
    /// * Contract MAY require caller to attach larger deposit, to cover cost of
    ///   storing approver data
    /// * Contract MUST panic if called by someone other than token owner
    /// * Contract MUST panic if addition would cause `nft_revoke_all` to exceed
    ///   single-block gas limit
    /// * Contract MUST increment approval ID even if re-approving an account
    /// * If successfully approved or if had already been approved, and if `msg` is
    ///   present, contract MUST call `nft_on_approve` on `account_id`. See
    ///   `nft_on_approve` description below for details.
    ///
    /// Arguments:
    /// * `token_id`: the token for which to add an approval
    /// * `account_id`: the account to add to `approvals`
    /// * `msg`: optional string to be passed to `nft_on_approve`
    ///
    /// Returns void, if no `msg` given. Otherwise, returns promise call to
    /// `nft_on_approve`, which can resolve with whatever it wants.
    fn nft_approve(
        &mut self,
        token_id: TokenId,
        account_id: AccountId,
        msg: Option<String>,
    ) -> Result<Option<Promise>, BaseError>;

    /// Revoke an approved account for a specific token.
    ///
    /// Requirements
    /// * Caller of the method must attach a deposit of 1 yoctoⓃ for security
    ///   purposes
    /// * If contract requires >1yN deposit on `nft_approve`, contract
    ///   MUST refund associated storage deposit when owner revokes approval
    /// * Contract MUST panic if called by someone other than token owner
    ///
    /// Arguments:
    /// * `token_id`: the token for which to revoke an approval
    /// * `account_id`: the account to remove from `approvals`
    fn nft_revoke(&mut self, token_id: TokenId, account_id: AccountId) -> Result<(), BaseError>;

    /// Revoke all approved accounts for a specific token.
    ///
    /// Requirements
    /// * Caller of the method must attach a deposit of 1 yoctoⓃ for security
    ///   purposes
    /// * If contract requires >1yN deposit on `nft_approve`, contract
    ///   MUST refund all associated storage deposit when owner revokes approvals
    /// * Contract MUST panic if called by someone other than token owner
    ///
    /// Arguments:
    /// * `token_id`: the token with approvals to revoke
    fn nft_revoke_all(&mut self, token_id: TokenId) -> Result<(), BaseError>;

    /// Check if a token is approved for transfer by a given account, optionally
    /// checking an approval_id
    ///
    /// Arguments:
    /// * `token_id`: the token for which to revoke an approval
    /// * `approved_account_id`: the account to check the existence of in `approvals`
    /// * `approval_id`: an optional approval ID to check against current approval ID for given account
    ///
    /// Returns:
    /// if `approval_id` given, `true` if `approved_account_id` is approved with given `approval_id`
    /// otherwise, `true` if `approved_account_id` is in list of approved accounts
    fn nft_is_approved(
        &self,
        token_id: TokenId,
        approved_account_id: AccountId,
        approval_id: Option<u64>,
    ) -> Result<bool, BaseError>;
}
