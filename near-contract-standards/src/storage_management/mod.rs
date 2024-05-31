use near_sdk::{near, AccountId, BaseError, NearToken};

#[near(serializers=[borsh, json])]
pub struct StorageBalance {
    pub total: NearToken,
    pub available: NearToken,
}

#[near(serializers=[borsh, json])]
pub struct StorageBalanceBounds {
    pub min: NearToken,
    pub max: Option<NearToken>,
}

/// Ensures that when fungible token storage grows by collections adding entries,
/// the storage is be paid by the caller. This ensures that storage cannot grow to a point
/// that the FT contract runs out of Ⓝ.
/// Takes name of the Contract struct, the inner field for the token and optional method name to
/// call when the account was closed.
///
/// # Examples
///
/// ```
/// use near_sdk::{near, PanicOnDefault, AccountId, NearToken, log, unwrap_or_err, BaseError};
/// use near_sdk::collections::LazyOption;
/// use near_sdk::json_types::U128;
/// use near_contract_standards::fungible_token::FungibleToken;
/// use near_contract_standards::storage_management::{
///     StorageBalance, StorageBalanceBounds, StorageManagement,
/// };
/// use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;
///
/// #[near(contract_state)]
/// #[derive(PanicOnDefault)]
/// pub struct Contract {
///     token: FungibleToken,
///     metadata: LazyOption<FungibleTokenMetadata>,
/// }
///
/// #[near]
/// impl StorageManagement for Contract {
///     #[payable]
///     fn storage_deposit(
///         &mut self,
///         account_id: Option<AccountId>,
///         registration_only: Option<bool>,
///     ) -> Result<StorageBalance, BaseError> {
///         self.token.storage_deposit(account_id, registration_only)
///     }
///
///     #[payable]
///     fn storage_withdraw(&mut self, amount: Option<NearToken>) -> Result<StorageBalance, BaseError> {
///         self.token.storage_withdraw(amount)
///     }
///
///     #[payable]
///     fn storage_unregister(&mut self, force: Option<bool>) -> Result<bool, BaseError> {
///         #[allow(unused_variables)]
///         if let Some((account_id, balance)) = unwrap_or_err!(self.token.internal_storage_unregister(force)) {
///             log!("Closed @{} with {}", account_id, balance);
///             Ok(true)
///         } else {
///             Ok(false)
///         }
///     }
///
///     fn storage_balance_bounds(&self) -> StorageBalanceBounds {
///         self.token.storage_balance_bounds()
///     }
///
///     fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
///         self.token.storage_balance_of(account_id)
///     }
/// }
///
/// ```
///
pub trait StorageManagement {
    // if `registration_only=true` MUST refund above the minimum balance if the account didn't exist and
    //     refund full deposit if the account exists.
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> Result<StorageBalance, BaseError>;

    /// Withdraw specified amount of available Ⓝ for predecessor account.
    ///
    /// This method is safe to call. It MUST NOT remove data.
    ///
    /// `amount` is sent as a string representing an unsigned 128-bit integer. If
    /// omitted, contract MUST refund full `available` balance. If `amount` exceeds
    /// predecessor account's available balance, contract MUST panic.
    ///
    /// If predecessor account not registered, contract MUST panic.
    ///
    /// MUST require exactly 1 yoctoNEAR attached balance to prevent restricted
    /// function-call access-key call (UX wallet security)
    ///
    /// Returns the StorageBalance structure showing updated balances.
    fn storage_withdraw(&mut self, amount: Option<NearToken>) -> Result<StorageBalance, BaseError>;

    /// Unregisters the predecessor account and returns the storage NEAR deposit back.
    ///
    /// If the predecessor account is not registered, the function MUST return `false` without panic.
    ///
    /// If `force=true` the function SHOULD ignore account balances (burn them) and close the account.
    /// Otherwise, MUST panic if caller has a positive registered balance (eg token holdings) or
    ///     the contract doesn't support force unregistration.
    /// MUST require exactly 1 yoctoNEAR attached balance to prevent restricted function-call access-key call
    /// (UX wallet security)
    /// Returns `true` iff the account was unregistered.
    /// Returns `false` iff account was not registered before.
    fn storage_unregister(&mut self, force: Option<bool>) -> Result<bool, BaseError>;

    fn storage_balance_bounds(&self) -> StorageBalanceBounds;

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance>;
}
