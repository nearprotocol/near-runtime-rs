mod core_impl;

mod receiver;
mod resolver;

pub use self::core_impl::*;

pub use self::receiver::NonFungibleTokenReceiver;
pub use self::resolver::NonFungibleTokenResolver;

use crate::non_fungible_token::token::{Token, TokenId};
use near_sdk::AccountId;
use near_sdk::BaseError;
use near_sdk::PromiseOrValue;

/// Used for all non-fungible tokens. The specification for the
/// [core non-fungible token standard] lays out the reasoning for each method.
/// It's important to check out [NonFungibleTokenReceiver](crate::non_fungible_token::core::NonFungibleTokenReceiver)
/// and [NonFungibleTokenResolver](crate::non_fungible_token::core::NonFungibleTokenResolver) to
/// understand how the cross-contract call work.
///
/// [core non-fungible token standard]: <https://nomicon.io/Standards/NonFungibleToken/Core.html>
///
/// # Examples
///
/// ```
/// use near_sdk::{PanicOnDefault, AccountId, PromiseOrValue, near, BaseError};
/// use near_contract_standards::non_fungible_token::{core::NonFungibleTokenCore, NonFungibleToken, TokenId, Token};
///
/// #[near(contract_state)]
/// #[derive(PanicOnDefault)]
/// pub struct Contract {
///    tokens: NonFungibleToken,
///}
/// #[near]
/// impl NonFungibleTokenCore for Contract {
///     #[payable]
///    fn nft_transfer(&mut self, receiver_id: AccountId, token_id: TokenId, approval_id: Option<u64>, memo: Option<String>) -> Result<(), BaseError> {
///        self.tokens.nft_transfer(receiver_id, token_id, approval_id, memo)
///    }
///
///    #[payable]
///    fn nft_transfer_call(&mut self, receiver_id: AccountId, token_id: TokenId, approval_id: Option<u64>, memo: Option<String>, msg: String) -> PromiseOrValue<bool> {
///        self.tokens.nft_transfer_call(receiver_id, token_id, approval_id, memo, msg)
///    }
///
///    fn nft_token(&self, token_id: TokenId) -> Option<Token> {
///        self.tokens.nft_token(token_id)
///    }
///}
/// ```
///
pub trait NonFungibleTokenCore {
    /// Simple transfer. Transfer a given `token_id` from current owner to
    /// `receiver_id`.
    ///
    /// Requirements
    /// * Caller of the method must attach a deposit of 1 yoctoⓃ for security purposes
    /// * Contract MUST panic if called by someone other than token owner or,
    ///   if using Approval Management, one of the approved accounts
    /// * `approval_id` is for use with Approval Management,
    ///   see <https://nomicon.io/Standards/NonFungibleToken/ApprovalManagement.html>
    /// * If using Approval Management, contract MUST nullify approved accounts on
    ///   successful transfer.
    /// * TODO: needed? Both accounts must be registered with the contract for transfer to
    ///   succeed. See see <https://nomicon.io/Standards/StorageManagement.html>
    ///
    /// Arguments:
    /// * `receiver_id`: the valid NEAR account receiving the token
    /// * `token_id`: the token to transfer
    /// * `approval_id`: expected approval ID. A number smaller than
    ///    2^53, and therefore representable as JSON. See Approval Management
    ///    standard for full explanation.
    /// * `memo` (optional): for use cases that may benefit from indexing or
    ///    providing information for a transfer
    fn nft_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) -> Result<(), BaseError>;

    /// Transfer token and call a method on a receiver contract. A successful
    /// workflow will end in a success execution outcome to the callback on the NFT
    /// contract at the method `nft_resolve_transfer`.
    ///
    /// You can think of this as being similar to attaching native NEAR tokens to a
    /// function call. It allows you to attach any Non-Fungible Token in a call to a
    /// receiver contract.
    ///
    /// Requirements:
    /// * Caller of the method must attach a deposit of 1 yoctoⓃ for security
    ///   purposes
    /// * Contract MUST panic if called by someone other than token owner or,
    ///   if using Approval Management, one of the approved accounts
    /// * The receiving contract must implement `ft_on_transfer` according to the
    ///   standard. If it does not, FT contract's `ft_resolve_transfer` MUST deal
    ///   with the resulting failed cross-contract call and roll back the transfer.
    /// * Contract MUST implement the behavior described in `ft_resolve_transfer`
    /// * `approval_id` is for use with Approval Management extension, see
    ///   that document for full explanation.
    /// * If using Approval Management, contract MUST nullify approved accounts on
    ///   successful transfer.
    ///
    /// Arguments:
    /// * `receiver_id`: the valid NEAR account receiving the token.
    /// * `token_id`: the token to send.
    /// * `approval_id`: expected approval ID. A number smaller than
    ///    2^53, and therefore representable as JSON. See Approval Management
    ///    standard for full explanation.
    /// * `memo` (optional): for use cases that may benefit from indexing or
    ///    providing information for a transfer.
    /// * `msg`: specifies information needed by the receiving contract in
    ///    order to properly handle the transfer. Can indicate both a function to
    ///    call and the parameters to pass to that function.
    fn nft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<bool>;

    /// Returns the token with the given `token_id` or `null` if no such token.
    fn nft_token(&self, token_id: TokenId) -> Option<Token>;
}
