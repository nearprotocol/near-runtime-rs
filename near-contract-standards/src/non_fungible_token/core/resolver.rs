use crate::non_fungible_token::token::TokenId;
use near_sdk::{ext_contract, AccountId, BaseError};
use std::collections::HashMap;

/// Used when an NFT is transferred using `nft_transfer_call`. This is the method that's called after `nft_on_transfer`. This trait is implemented on the NFT contract.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use near_sdk::{PanicOnDefault, AccountId, PromiseOrValue, near, BaseError};
/// use near_contract_standards::non_fungible_token::{NonFungibleToken, NonFungibleTokenResolver, TokenId};
///
/// #[near(contract_state)]
/// #[derive(PanicOnDefault)]
/// pub struct Contract {
///    tokens: NonFungibleToken,
///}
/// #[near]
/// impl NonFungibleTokenResolver for Contract {
///     #[private]
///     fn nft_resolve_transfer(&mut self, previous_owner_id: AccountId, receiver_id: AccountId, token_id: TokenId, approved_account_ids: Option<HashMap<AccountId, u64>>) -> Result<bool, BaseError> {
///         self.tokens.nft_resolve_transfer(previous_owner_id, receiver_id, token_id, approved_account_ids)
///     }
/// }
/// ```
///
#[ext_contract(ext_nft_resolver)]
pub trait NonFungibleTokenResolver {
    /// Finalize an `nft_transfer_call` chain of cross-contract calls.
    ///
    /// The `nft_transfer_call` process:
    ///
    /// 1. Sender calls `nft_transfer_call` on FT contract
    /// 2. NFT contract transfers token from sender to receiver
    /// 3. NFT contract calls `nft_on_transfer` on receiver contract
    /// 4+. [receiver contract may make other cross-contract calls]
    /// N. NFT contract resolves promise chain with `nft_resolve_transfer`, and may
    ///    transfer token back to sender
    ///
    /// Requirements:
    /// * Contract MUST forbid calls to this function by any account except self
    /// * If promise chain failed, contract MUST revert token transfer
    /// * If promise chain resolves with `true`, contract MUST return token to
    ///   `sender_id`
    ///
    /// Arguments:
    /// * `previous_owner_id`: the owner prior to the call to `nft_transfer_call`
    /// * `receiver_id`: the `receiver_id` argument given to `nft_transfer_call`
    /// * `token_id`: the `token_id` argument given to `ft_transfer_call`
    /// * `approved_account_ids`: if using Approval Management, contract MUST provide
    ///   set of original approved accounts in this argument, and restore these
    ///   approved accounts in case of revert.
    ///
    /// Returns true if token was successfully transferred to `receiver_id`.
    fn nft_resolve_transfer(
        &mut self,
        previous_owner_id: AccountId,
        receiver_id: AccountId,
        token_id: TokenId,
        approved_account_ids: Option<HashMap<AccountId, u64>>,
    ) -> Result<bool, BaseError>;
}
