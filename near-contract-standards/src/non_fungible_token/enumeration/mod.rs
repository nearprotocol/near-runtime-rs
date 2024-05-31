mod enumeration_impl;

use crate::non_fungible_token::token::Token;
use near_sdk::json_types::U128;
use near_sdk::{AccountId, BaseError};

/// Offers methods helpful in determining account ownership of NFTs and provides a way to page through NFTs per owner, determine total supply, etc.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use near_sdk::{PanicOnDefault, AccountId, PromiseOrValue, near, Promise, BaseError};
/// use near_contract_standards::non_fungible_token::{NonFungibleToken, NonFungibleTokenEnumeration, TokenId, Token};
/// use near_sdk::json_types::U128;
///
/// #[near(contract_state)]
/// #[derive(PanicOnDefault)]
/// pub struct Contract {
///    tokens: NonFungibleToken,
///}
///
/// #[near]
/// impl NonFungibleTokenEnumeration for Contract {
///     fn nft_total_supply(&self) -> U128 {
///         self.tokens.nft_total_supply()
///     }
///
///     fn nft_tokens(&self, from_index: Option<U128>, limit: Option<u64>) -> Result<Vec<Token>, BaseError> {
///         self.tokens.nft_tokens(from_index, limit)
///     }
///
///     fn nft_supply_for_owner(&self, account_id: AccountId) -> Result<U128, BaseError> {
///         self.tokens.nft_supply_for_owner(account_id)
///     }
///
///     fn nft_tokens_for_owner(&self, account_id: AccountId, from_index: Option<U128>, limit: Option<u64>) -> Result<Vec<Token>, BaseError> {
///         self.tokens.nft_tokens_for_owner(account_id, from_index, limit)
///     }
/// }
/// ```
///
pub trait NonFungibleTokenEnumeration {
    /// Returns the total supply of non-fungible tokens as a string representing an
    /// unsigned 128-bit integer to avoid JSON number limit of 2^53.
    fn nft_total_supply(&self) -> U128;

    /// Get a list of all tokens
    ///
    /// Arguments:
    /// * `from_index`: a string representing an unsigned 128-bit integer,
    ///    representing the starting index of tokens to return
    /// * `limit`: the maximum number of tokens to return
    ///
    /// Returns an array of Token objects, as described in Core standard
    fn nft_tokens(
        &self,
        from_index: Option<U128>, // default: "0"
        limit: Option<u64>,       // default: unlimited (could fail due to gas limit)
    ) -> Result<Vec<Token>, BaseError>;

    /// Get number of tokens owned by a given account
    ///
    /// Arguments:
    /// * `account_id`: a valid NEAR account
    ///
    /// Returns the number of non-fungible tokens owned by given `account_id` as
    /// a string representing the value as an unsigned 128-bit integer to avoid JSON
    /// number limit of 2^53.
    fn nft_supply_for_owner(&self, account_id: AccountId) -> Result<U128, BaseError>;

    /// Get list of all tokens owned by a given account
    ///
    /// Arguments:
    /// * `account_id`: a valid NEAR account
    /// * `from_index`: a string representing an unsigned 128-bit integer,
    ///    representing the starting index of tokens to return
    /// * `limit`: the maximum number of tokens to return
    ///
    /// Returns a paginated list of all tokens owned by this account
    fn nft_tokens_for_owner(
        &self,
        account_id: AccountId,
        from_index: Option<U128>, // default: "0"
        limit: Option<u64>,       // default: unlimited (could fail due to gas limit)
    ) -> Result<Vec<Token>, BaseError>;
}
