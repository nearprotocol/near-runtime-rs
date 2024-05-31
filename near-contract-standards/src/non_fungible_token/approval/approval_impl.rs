use crate::non_fungible_token::approval::ext_nft_approval_receiver;
/// Common implementation of the [approval management standard](https://nomicon.io/Standards/NonFungibleToken/ApprovalManagement.html) for NFTs.
/// on the contract/account that has just been approved. This is not required to implement.
use crate::non_fungible_token::approval::NonFungibleTokenApproval;
use crate::non_fungible_token::token::TokenId;
use crate::non_fungible_token::utils::{
    assert_at_least_one_yocto, bytes_for_approved_account_id, refund_approved_account_ids,
    refund_approved_account_ids_iter, refund_deposit,
};
use crate::non_fungible_token::ApprovalNotSupported;
use crate::non_fungible_token::NonFungibleToken;
use near_sdk::errors::PermissionDenied;
use near_sdk::{
    assert_one_yocto, contract_error, env, require_or_err, unwrap_or_err, AccountId, BaseError,
    Gas, Promise,
};

const GAS_FOR_NFT_APPROVE: Gas = Gas::from_tgas(10);

#[contract_error]
pub struct TokenNotFound {}

fn expect_token_found<T>(option: Option<T>) -> Result<T, TokenNotFound> {
    Ok(unwrap_or_err!(option, TokenNotFound {}))
}

#[contract_error]
pub struct TokenNotApproved {
    message: String,
}

impl TokenNotApproved {
    pub fn new(message: &str) -> Self {
        Self { message: String::from(message) }
    }
}

fn expect_approval<T>(option: Option<T>) -> Result<T, TokenNotApproved> {
    Ok(unwrap_or_err!(
        option,
        TokenNotApproved::new("next_approval_by_id must be set for approval ext")
    ))
}

impl NonFungibleTokenApproval for NonFungibleToken {
    fn nft_approve(
        &mut self,
        token_id: TokenId,
        account_id: AccountId,
        msg: Option<String>,
    ) -> Result<Option<Promise>, BaseError> {
        unwrap_or_err!(assert_at_least_one_yocto());
        let approvals_by_id = unwrap_or_err!(
            self.approvals_by_id.as_mut(),
            ApprovalNotSupported::new("NFT does not support Approval Management")
        );

        let owner_id = unwrap_or_err!(expect_token_found(self.owner_by_id.get(&token_id)));

        require_or_err!(
            env::predecessor_account_id() == owner_id,
            PermissionDenied::new(Some("Predecessor must be token owner."))
        );

        let next_approval_id_by_id =
            unwrap_or_err!(expect_approval(self.next_approval_id_by_id.as_mut()));
        // update HashMap of approvals for this token
        let approved_account_ids = &mut approvals_by_id.get(&token_id).unwrap_or_default();
        let approval_id: u64 = next_approval_id_by_id.get(&token_id).unwrap_or(1u64);
        let old_approval_id = approved_account_ids.insert(account_id.clone(), approval_id);

        // save updated approvals HashMap to contract's LookupMap
        approvals_by_id.insert(&token_id, approved_account_ids);

        // increment next_approval_id for this token
        next_approval_id_by_id.insert(&token_id, &(approval_id + 1));

        // If this approval replaced existing for same account, no storage was used.
        // Otherwise, require that enough deposit was attached to pay for storage, and refund
        // excess.
        let storage_used =
            if old_approval_id.is_none() { bytes_for_approved_account_id(&account_id) } else { 0 };
        unwrap_or_err!(refund_deposit(storage_used));

        // if given `msg`, schedule call to `nft_on_approve` and return it. Else, return None.
        Ok(msg.map(|msg| {
            ext_nft_approval_receiver::ext(account_id)
                .with_static_gas(env::prepaid_gas().saturating_sub(GAS_FOR_NFT_APPROVE))
                .nft_on_approve(token_id, owner_id, approval_id, msg)
        }))
    }

    fn nft_revoke(&mut self, token_id: TokenId, account_id: AccountId) -> Result<(), BaseError> {
        assert_one_yocto();
        let approvals_by_id = unwrap_or_err!(
            self.approvals_by_id.as_mut(),
            ApprovalNotSupported::new("NFT does not support Approval Management")
        );

        let owner_id = unwrap_or_err!(expect_token_found(self.owner_by_id.get(&token_id)));
        let predecessor_account_id = env::predecessor_account_id();

        require_or_err!(
            predecessor_account_id == owner_id,
            PermissionDenied::new(Some("Predecessor must be token owner."))
        );

        // if token has no approvals, do nothing
        if let Some(approved_account_ids) = &mut approvals_by_id.get(&token_id) {
            // if account_id was already not approved, do nothing
            if approved_account_ids.remove(&account_id).is_some() {
                refund_approved_account_ids_iter(
                    predecessor_account_id,
                    core::iter::once(&account_id),
                );
                // if this was the last approval, remove the whole HashMap to save space.
                if approved_account_ids.is_empty() {
                    approvals_by_id.remove(&token_id);
                } else {
                    // otherwise, update approvals_by_id with updated HashMap
                    approvals_by_id.insert(&token_id, approved_account_ids);
                }
            }
        }
        Ok(())
    }

    fn nft_revoke_all(&mut self, token_id: TokenId) -> Result<(), BaseError> {
        assert_one_yocto();
        let approvals_by_id = unwrap_or_err!(
            self.approvals_by_id.as_mut(),
            ApprovalNotSupported::new("NFT does not support Approval Management")
        );

        let owner_id = unwrap_or_err!(expect_token_found(self.owner_by_id.get(&token_id)));
        let predecessor_account_id = env::predecessor_account_id();

        require_or_err!(
            predecessor_account_id == owner_id,
            PermissionDenied::new(Some("Predecessor must be token owner."))
        );

        // if token has no approvals, do nothing
        if let Some(approved_account_ids) = &mut approvals_by_id.get(&token_id) {
            // otherwise, refund owner for storage costs of all approvals...
            refund_approved_account_ids(predecessor_account_id, approved_account_ids);
            // ...and remove whole HashMap of approvals
            approvals_by_id.remove(&token_id);
        }
        Ok(())
    }

    fn nft_is_approved(
        &self,
        token_id: TokenId,
        approved_account_id: AccountId,
        approval_id: Option<u64>,
    ) -> Result<bool, BaseError> {
        unwrap_or_err!(expect_token_found(self.owner_by_id.get(&token_id)));

        let approvals_by_id = if let Some(a) = self.approvals_by_id.as_ref() {
            a
        } else {
            // contract does not support approval management
            return Ok(false);
        };

        let approved_account_ids = if let Some(ids) = approvals_by_id.get(&token_id) {
            ids
        } else {
            // token has no approvals
            return Ok(false);
        };

        let actual_approval_id = if let Some(id) = approved_account_ids.get(&approved_account_id) {
            id
        } else {
            // account not in approvals HashMap
            return Ok(false);
        };

        if let Some(given_approval_id) = approval_id {
            Ok(&given_approval_id == actual_approval_id)
        } else {
            // account approved, no approval_id given
            Ok(true)
        }
    }
}
