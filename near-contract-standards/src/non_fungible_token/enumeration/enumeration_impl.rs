use super::NonFungibleTokenEnumeration;
use crate::non_fungible_token::token::Token;
use crate::non_fungible_token::NonFungibleToken;
use near_sdk::errors::{IndexOutOfBounds, InvalidArgument};
use near_sdk::json_types::U128;
use near_sdk::{contract_error, require_or_err, unwrap_or_err, AccountId, BaseError};

type TokenId = String;

impl NonFungibleToken {
    /// Helper function used by a enumerations methods
    /// Note: this method is not exposed publicly to end users
    fn enum_get_token(&self, owner_id: AccountId, token_id: TokenId) -> Token {
        let metadata = self.token_metadata_by_id.as_ref().and_then(|m| m.get(&token_id));
        let approved_account_ids = self
            .approvals_by_id
            .as_ref()
            .map(|approvals_by_id| approvals_by_id.get(&token_id.to_string()).unwrap_or_default());

        Token { token_id, owner_id, metadata, approved_account_ids }
    }
}

impl NonFungibleTokenEnumeration for NonFungibleToken {
    fn nft_total_supply(&self) -> U128 {
        // An unfortunate cast from the max of TreeMap to the spec
        (self.owner_by_id.len() as u128).into()
    }

    fn nft_tokens(
        &self,
        from_index: Option<U128>,
        limit: Option<u64>,
    ) -> Result<Vec<Token>, BaseError> {
        // Get starting index, whether or not it was explicitly given.
        // Defaults to 0 based on the spec:
        // https://nomicon.io/Standards/NonFungibleToken/Enumeration.html#interface
        let start_index: u128 = from_index.map(From::from).unwrap_or_default();
        require_or_err!((self.owner_by_id.len() as u128) >= start_index, IndexOutOfBounds {});
        let limit = limit.map(|v| v as usize).unwrap_or(usize::MAX);
        require_or_err!(limit != 0, InvalidArgument::new("Cannot provide limit of 0."));
        Ok(self
            .owner_by_id
            .iter()
            .skip(start_index as usize)
            .take(limit)
            .map(|(token_id, owner_id)| self.enum_get_token(owner_id, token_id))
            .collect())
    }

    fn nft_supply_for_owner(&self, account_id: AccountId) -> Result<U128, BaseError> {
        let tokens_per_owner =
            unwrap_or_err!(self.tokens_per_owner.as_ref(), TokensNotFound::new());
        Ok(tokens_per_owner
            .get(&account_id)
            .map(|account_tokens| U128::from(account_tokens.len() as u128))
            .unwrap_or(U128(0)))
    }

    fn nft_tokens_for_owner(
        &self,
        account_id: AccountId,
        from_index: Option<U128>,
        limit: Option<u64>,
    ) -> Result<Vec<Token>, BaseError> {
        let tokens_per_owner =
            unwrap_or_err!(self.tokens_per_owner.as_ref(), TokensNotFound::new());
        let token_set = if let Some(token_set) = tokens_per_owner.get(&account_id) {
            token_set
        } else {
            return Ok(vec![]);
        };

        if token_set.is_empty() {
            return Ok(vec![]);
        }

        let limit = limit.map(|v| v as usize).unwrap_or(usize::MAX);
        require_or_err!(limit != 0, InvalidArgument::new("Cannot provide limit of 0."));
        let start_index: u128 = from_index.map(From::from).unwrap_or_default();
        require_or_err!(token_set.len() as u128 > start_index, IndexOutOfBounds {});
        Ok(token_set
            .iter()
            .skip(start_index as usize)
            .take(limit)
            .map(|token_id| self.enum_get_token(account_id.clone(), token_id))
            .collect())
    }
}

#[contract_error]
pub struct TokensNotFound {
    pub message: String,
}

impl TokensNotFound {
    pub fn new() -> Self {
        Self {
            message: "Could not find tokens_per_owner when calling a method on the \
        enumeration standard."
                .to_string(),
        }
    }
}

impl Default for TokensNotFound {
    fn default() -> Self {
        Self::new()
    }
}
