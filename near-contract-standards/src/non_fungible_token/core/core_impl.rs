use super::resolver::NonFungibleTokenResolver;
use crate::non_fungible_token::core::receiver::ext_nft_receiver;
use crate::non_fungible_token::core::resolver::ext_nft_resolver;
use crate::non_fungible_token::core::NonFungibleTokenCore;
use crate::non_fungible_token::events::{NftMint, NftTransfer};
use crate::non_fungible_token::metadata::TokenMetadata;
use crate::non_fungible_token::token::{Token, TokenId};
use crate::non_fungible_token::utils::{refund_approved_account_ids, refund_deposit_to_account};
use crate::non_fungible_token::ApprovalNotSupported;
use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::{LookupMap, TreeMap, UnorderedSet};
use near_sdk::errors::{InsufficientGas, InvalidArgument, PermissionDenied};
use near_sdk::json_types::Base64VecU8;
use near_sdk::{
    assert_one_yocto, contract_error, env, near, require, require_or_err, unwrap_or_err, AccountId,
    BaseError, BorshStorageKey, Gas, IntoStorageKey, PromiseOrValue, PromiseResult, StorageUsage,
};
use std::collections::HashMap;
use std::ops::Deref;

const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas::from_tgas(5);
const GAS_FOR_NFT_TRANSFER_CALL: Gas = Gas::from_tgas(30);

/// Implementation of the non-fungible token standard.
/// Allows to include NEP-171 compatible token to any contract.
/// There are next traits that any contract may implement:
///     - NonFungibleTokenCore -- interface with nft_transfer methods. NonFungibleToken provides methods for it.
///     - NonFungibleTokenApproval -- interface with nft_approve methods. NonFungibleToken provides methods for it.
///     - NonFungibleTokenEnumeration -- interface for getting lists of tokens. NonFungibleToken provides methods for it.
///     - NonFungibleTokenMetadata -- return metadata for the token in NEP-177, up to contract to implement.
///
/// For example usage, see examples/non-fungible-token/src/lib.rs.
#[near]
pub struct NonFungibleToken {
    // owner of contract
    pub owner_id: AccountId,

    // The storage size in bytes for each new token
    pub extra_storage_in_bytes_per_token: StorageUsage,

    // always required
    pub owner_by_id: TreeMap<TokenId, AccountId>,

    // required by metadata extension
    pub token_metadata_by_id: Option<LookupMap<TokenId, TokenMetadata>>,

    // required by enumeration extension
    pub tokens_per_owner: Option<LookupMap<AccountId, UnorderedSet<TokenId>>>,

    // required by approval extension
    pub approvals_by_id: Option<LookupMap<TokenId, HashMap<AccountId, u64>>>,
    pub next_approval_id_by_id: Option<LookupMap<TokenId, u64>>,
}

#[derive(BorshStorageKey, BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
pub enum StorageKey {
    TokensPerOwner { account_hash: Vec<u8> },
}

impl NonFungibleToken {
    pub fn new<Q, R, S, T>(
        owner_by_id_prefix: Q,
        owner_id: AccountId,
        token_metadata_prefix: Option<R>,
        enumeration_prefix: Option<S>,
        approval_prefix: Option<T>,
    ) -> Self
    where
        Q: IntoStorageKey,
        R: IntoStorageKey,
        S: IntoStorageKey,
        T: IntoStorageKey,
    {
        let (approvals_by_id, next_approval_id_by_id) = if let Some(prefix) = approval_prefix {
            let prefix: Vec<u8> = prefix.into_storage_key();
            (
                Some(LookupMap::new(prefix.clone())),
                Some(LookupMap::new([prefix, "n".into()].concat())),
            )
        } else {
            (None, None)
        };

        let mut this = Self {
            owner_id,
            extra_storage_in_bytes_per_token: 0,
            owner_by_id: TreeMap::new(owner_by_id_prefix),
            token_metadata_by_id: token_metadata_prefix.map(LookupMap::new),
            tokens_per_owner: enumeration_prefix.map(LookupMap::new),
            approvals_by_id,
            next_approval_id_by_id,
        };
        this.measure_min_token_storage_cost();
        this
    }

    // TODO: does this seem reasonable?
    fn measure_min_token_storage_cost(&mut self) {
        let initial_storage_usage = env::storage_usage();
        // 64 Length because this is the max account id length
        let tmp_token_id = "a".repeat(64);
        let tmp_owner_id = "a".repeat(64).parse().unwrap();

        // 1. set some dummy data
        self.owner_by_id.insert(&tmp_token_id, &tmp_owner_id);
        if let Some(token_metadata_by_id) = &mut self.token_metadata_by_id {
            token_metadata_by_id.insert(
                &tmp_token_id,
                &TokenMetadata {
                    title: Some("a".repeat(64)),
                    description: Some("a".repeat(64)),
                    media: Some("a".repeat(64)),
                    media_hash: Some(Base64VecU8::from("a".repeat(64).as_bytes().to_vec())),
                    copies: Some(1),
                    issued_at: None,
                    expires_at: None,
                    starts_at: None,
                    updated_at: None,
                    extra: None,
                    reference: None,
                    reference_hash: None,
                },
            );
        }
        if let Some(tokens_per_owner) = &mut self.tokens_per_owner {
            let u = &mut UnorderedSet::new(StorageKey::TokensPerOwner {
                account_hash: env::sha256(tmp_owner_id.as_bytes()),
            });
            u.insert(&tmp_token_id);
            tokens_per_owner.insert(&tmp_owner_id, u);
        }
        if let Some(approvals_by_id) = &mut self.approvals_by_id {
            let mut approvals = HashMap::new();
            approvals.insert(tmp_owner_id.clone(), 1u64);
            approvals_by_id.insert(&tmp_token_id, &approvals);
        }
        if let Some(next_approval_id_by_id) = &mut self.next_approval_id_by_id {
            next_approval_id_by_id.insert(&tmp_token_id, &1u64);
        }

        // 2. see how much space it took
        self.extra_storage_in_bytes_per_token = env::storage_usage() - initial_storage_usage;

        // 3. roll it all back
        if let Some(next_approval_id_by_id) = &mut self.next_approval_id_by_id {
            next_approval_id_by_id.remove(&tmp_token_id);
        }
        if let Some(approvals_by_id) = &mut self.approvals_by_id {
            approvals_by_id.remove(&tmp_token_id);
        }
        if let Some(tokens_per_owner) = &mut self.tokens_per_owner {
            let mut u = tokens_per_owner.remove(&tmp_owner_id).unwrap();
            u.remove(&tmp_token_id);
        }
        if let Some(token_metadata_by_id) = &mut self.token_metadata_by_id {
            token_metadata_by_id.remove(&tmp_token_id);
        }
        self.owner_by_id.remove(&tmp_token_id);
    }

    /// Transfer token_id from `from` to `to`
    ///
    /// Do not perform any safety checks or do any logging
    pub fn internal_transfer_unguarded(
        &mut self,
        #[allow(clippy::ptr_arg)] token_id: &TokenId,
        from: &AccountId,
        to: &AccountId,
    ) -> Result<(), BaseError> {
        // update owner
        self.owner_by_id.insert(token_id, to);

        // if using Enumeration standard, update old & new owner's token lists
        if let Some(tokens_per_owner) = &mut self.tokens_per_owner {
            // owner_tokens should always exist, so call `unwrap` without guard
            let owner_tokens = tokens_per_owner.get(from);
            match owner_tokens {
                Some(mut owner_tokens) => {
                    owner_tokens.remove(token_id);
                    if owner_tokens.is_empty() {
                        tokens_per_owner.remove(from);
                    } else {
                        tokens_per_owner.insert(from, &owner_tokens);
                    }
                }
                None => {
                    return Err(PermissionDenied::new(Some("Owner does not have token")).into());
                }
            }

            let mut receiver_tokens = tokens_per_owner.get(to).unwrap_or_else(|| {
                UnorderedSet::new(StorageKey::TokensPerOwner {
                    account_hash: env::sha256(to.as_bytes()),
                })
            });
            receiver_tokens.insert(token_id);
            tokens_per_owner.insert(to, &receiver_tokens);
            return Ok(());
        }
        Ok(())
    }

    /// Transfer from current owner to receiver_id, checking that sender is allowed to transfer.
    /// Clear approvals, if approval extension being used.
    /// Return previous owner and approvals.
    pub fn internal_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        #[allow(clippy::ptr_arg)] token_id: &TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) -> Result<(AccountId, Option<HashMap<AccountId, u64>>), BaseError> {
        let owner_id = unwrap_or_err!(self.owner_by_id.get(token_id), TokenNotFound {});

        // clear approvals, if using Approval Management extension
        // this will be rolled back by a panic if sending fails
        let approved_account_ids =
            self.approvals_by_id.as_mut().map(|by_id| by_id.remove(token_id).unwrap_or_default());

        // check if authorized
        let sender_id = if sender_id != &owner_id {
            // Panic if approval extension is NOT being used
            let app_acc_ids = unwrap_or_err!(
                approved_account_ids.as_ref(),
                ApprovalNotSupported::new("Approval extension is disabled")
            );

            // Approval extension is being used; get approval_id for sender.
            let actual_approval_id = app_acc_ids.get(sender_id);

            // Panic if sender not approved at all
            if actual_approval_id.is_none() {
                return Err(PermissionDenied::new(Some("Sender not approved")).into());
            }

            // If approval_id included, check that it matches
            require_or_err!(
                approval_id.is_none() || actual_approval_id == approval_id.as_ref(),
                PermissionDenied::new(Some(
                    format!(
                        "The actual approval_id {:?} is different from the given approval_id {:?}",
                        actual_approval_id, approval_id
                    )
                    .as_str()
                ))
            );
            Some(sender_id)
        } else {
            None
        };

        require_or_err!(&owner_id != receiver_id, ReceiverIsSender::new());

        unwrap_or_err!(self.internal_transfer_unguarded(token_id, &owner_id, receiver_id));

        NonFungibleToken::emit_transfer(&owner_id, receiver_id, token_id, sender_id, memo);

        // return previous owner & approvals
        Ok((owner_id, approved_account_ids))
    }

    fn emit_transfer(
        owner_id: &AccountId,
        receiver_id: &AccountId,
        token_id: &str,
        sender_id: Option<&AccountId>,
        memo: Option<String>,
    ) {
        NftTransfer {
            old_owner_id: owner_id,
            new_owner_id: receiver_id,
            token_ids: &[token_id],
            authorized_id: sender_id.filter(|sender_id| *sender_id == owner_id).map(|f| f.deref()),
            memo: memo.as_deref(),
        }
        .emit();
    }

    /// Mint a new token. Not part of official standard, but needed in most situations.
    /// Consuming contract expected to wrap this with an `nft_mint` function.
    ///
    /// Requirements:
    /// * Caller must be the `owner_id` set during contract initialization.
    /// * Caller of the method must attach a deposit of 1 yoctoⓃ for security purposes.
    /// * If contract is using Metadata extension (by having provided `metadata_prefix` during
    ///   contract initialization), `token_metadata` must be given.
    /// * token_id must be unique
    ///
    /// Returns the newly minted token
    #[deprecated(since = "4.0.0", note = "mint is deprecated, please use internal_mint instead.")]
    pub fn mint(
        &mut self,
        token_id: TokenId,
        token_owner_id: AccountId,
        token_metadata: Option<TokenMetadata>,
    ) -> Result<Token, BaseError> {
        assert_eq!(env::predecessor_account_id(), self.owner_id, "Unauthorized");

        self.internal_mint(token_id, token_owner_id, token_metadata)
    }

    /// Mint a new token without checking:
    /// * Whether the caller id is equal to the `owner_id`
    /// * Assumes there will be a refund to the predecessor after covering the storage costs
    ///
    /// Returns the newly minted token and emits the mint event
    pub fn internal_mint(
        &mut self,
        token_id: TokenId,
        token_owner_id: AccountId,
        token_metadata: Option<TokenMetadata>,
    ) -> Result<Token, BaseError> {
        let token = self.internal_mint_with_refund(
            token_id,
            token_owner_id,
            token_metadata,
            Some(env::predecessor_account_id()),
        );

        match token {
            Ok(token) => {
                NftMint { owner_id: &token.owner_id, token_ids: &[&token.token_id], memo: None }
                    .emit();
                Ok(token)
            }
            Err(err) => Err(err),
        }
    }

    /// Mint a new token without checking:
    /// * Whether the caller id is equal to the `owner_id`
    /// * `refund_id` will transfer the left over balance after storage costs are calculated to the provided account.
    ///   Typically the account will be the owner. If `None`, will not refund. This is useful for delaying refunding
    ///   until multiple tokens have been minted.
    ///
    /// Returns the newly minted token and does not emit the mint event. This allows minting multiple before emitting.
    pub fn internal_mint_with_refund(
        &mut self,
        token_id: TokenId,
        token_owner_id: AccountId,
        token_metadata: Option<TokenMetadata>,
        refund_id: Option<AccountId>,
    ) -> Result<Token, BaseError> {
        // Remember current storage usage if refund_id is Some
        let initial_storage_usage = refund_id.map(|account_id| (account_id, env::storage_usage()));

        if self.token_metadata_by_id.is_some() && token_metadata.is_none() {
            return Err(InvalidArgument::new("Must provide metadata").into());
        }
        if self.owner_by_id.get(&token_id).is_some() {
            return Err(InvalidArgument::new("token_id must be unique").into());
        }

        let owner_id: AccountId = token_owner_id;

        // Core behavior: every token must have an owner
        self.owner_by_id.insert(&token_id, &owner_id);

        // Metadata extension: Save metadata, keep variable around to return later.
        // Note that check above already panicked if metadata extension in use but no metadata
        // provided to call.
        self.token_metadata_by_id
            .as_mut()
            .and_then(|by_id| by_id.insert(&token_id, token_metadata.as_ref().unwrap()));

        // Enumeration extension: Record tokens_per_owner for use with enumeration view methods.
        if let Some(tokens_per_owner) = &mut self.tokens_per_owner {
            let mut token_ids = tokens_per_owner.get(&owner_id).unwrap_or_else(|| {
                UnorderedSet::new(StorageKey::TokensPerOwner {
                    account_hash: env::sha256(owner_id.as_bytes()),
                })
            });
            token_ids.insert(&token_id);
            tokens_per_owner.insert(&owner_id, &token_ids);
        }

        // Approval Management extension: return empty HashMap as part of Token
        let approved_account_ids =
            if self.approvals_by_id.is_some() { Some(HashMap::new()) } else { None };

        if let Some((id, storage_usage)) = initial_storage_usage {
            unwrap_or_err!(refund_deposit_to_account(env::storage_usage() - storage_usage, id))
        }

        // Return any extra attached deposit not used for storage

        Ok(Token { token_id, owner_id, metadata: token_metadata, approved_account_ids })
    }
}

#[contract_error]
pub struct ReceiverIsSender {
    pub message: String,
}

impl ReceiverIsSender {
    pub fn new() -> Self {
        Self { message: "Current and next owner must differ".to_string() }
    }
}

impl Default for ReceiverIsSender {
    fn default() -> Self {
        Self::new()
    }
}

#[contract_error]
pub struct TokenNotFound {}

impl NonFungibleTokenCore for NonFungibleToken {
    fn nft_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) -> Result<(), BaseError> {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        unwrap_or_err!(self.internal_transfer(
            &sender_id,
            &receiver_id,
            &token_id,
            approval_id,
            memo
        ));
        Ok(())
    }

    fn nft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<bool> {
        assert_one_yocto();
        require!(env::prepaid_gas() > GAS_FOR_NFT_TRANSFER_CALL, &String::from(InsufficientGas {}));
        let sender_id = env::predecessor_account_id();
        let (old_owner, old_approvals) = self
            .internal_transfer(&sender_id, &receiver_id, &token_id, approval_id, memo)
            .unwrap_or_else(|err| env::panic_err(err));
        // Initiating receiver's call and the callback
        ext_nft_receiver::ext(receiver_id.clone())
            .with_static_gas(env::prepaid_gas().saturating_sub(GAS_FOR_NFT_TRANSFER_CALL))
            .nft_on_transfer(sender_id, old_owner.clone(), token_id.clone(), msg)
            .then(
                ext_nft_resolver::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                    .nft_resolve_transfer(old_owner, receiver_id, token_id, old_approvals),
            )
            .into()
    }

    fn nft_token(&self, token_id: TokenId) -> Option<Token> {
        let owner_id = self.owner_by_id.get(&token_id)?;
        let metadata = self.token_metadata_by_id.as_ref().and_then(|by_id| by_id.get(&token_id));
        let approved_account_ids = self
            .approvals_by_id
            .as_ref()
            .and_then(|by_id| by_id.get(&token_id).or_else(|| Some(HashMap::new())));
        Some(Token { token_id, owner_id, metadata, approved_account_ids })
    }
}

impl NonFungibleTokenResolver for NonFungibleToken {
    /// Returns true if token was successfully transferred to `receiver_id`.
    fn nft_resolve_transfer(
        &mut self,
        previous_owner_id: AccountId,
        receiver_id: AccountId,
        token_id: TokenId,
        approved_account_ids: Option<HashMap<AccountId, u64>>,
    ) -> Result<bool, BaseError> {
        // Get whether token should be returned
        let must_revert = match env::promise_result(0) {
            PromiseResult::Successful(value) => {
                if let Ok(yes_or_no) = near_sdk::serde_json::from_slice::<bool>(&value) {
                    yes_or_no
                } else {
                    true
                }
            }
            PromiseResult::Failed => true,
        };

        // if call succeeded, return early
        if !must_revert {
            return Ok(true);
        }

        // OTHERWISE, try to set owner back to previous_owner_id and restore approved_account_ids

        // Check that receiver didn't already transfer it away or burn it.
        if let Some(current_owner) = self.owner_by_id.get(&token_id) {
            if current_owner != receiver_id {
                // The token is not owned by the receiver anymore. Can't return it.
                return Ok(true);
            }
        } else {
            // The token was burned and doesn't exist anymore.
            // Refund storage cost for storing approvals to original owner and return early.
            if let Some(approved_account_ids) = approved_account_ids {
                refund_approved_account_ids(previous_owner_id, &approved_account_ids);
            }
            return Ok(true);
        };

        unwrap_or_err!(self.internal_transfer_unguarded(
            &token_id,
            &receiver_id,
            &previous_owner_id
        ));

        // If using Approval Management extension,
        // 1. revert any approvals receiver already set, refunding storage costs
        // 2. reset approvals to what previous owner had set before call to nft_transfer_call
        if let Some(by_id) = &mut self.approvals_by_id {
            if let Some(receiver_approvals) = by_id.remove(&token_id) {
                refund_approved_account_ids(receiver_id.clone(), &receiver_approvals);
            }
            if let Some(previous_owner_approvals) = approved_account_ids {
                by_id.insert(&token_id, &previous_owner_approvals);
            }
        }
        NonFungibleToken::emit_transfer(&receiver_id, &previous_owner_id, &token_id, None, None);
        Ok(false)
    }
}
