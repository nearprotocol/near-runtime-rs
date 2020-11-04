use crate::runtime::init_runtime;
pub use crate::to_yocto;
use crate::{
    account::{AccessKey, Account},
    hash::CryptoHash,
    outcome_into_result,
    runtime::{GenesisConfig, RuntimeStandalone},
    transaction::Transaction,
    types::{AccountId, Balance, Gas},
    ExecutionResult, ViewResult,
};
use near_crypto::{InMemorySigner, KeyType, Signer};
use near_sdk::PendingContractTx;
use std::{cell::RefCell, rc::Rc};

pub const DEFAULT_GAS: u64 = 300_000_000_000_000;
pub const STORAGE_AMOUNT: u128 = 50_000_000_000_000_000_000_000_000;

pub struct UserAccount {
    runtime: Rc<RefCell<RuntimeStandalone>>,
    pub account_id: AccountId,
    pub signer: InMemorySigner,
}

impl UserAccount {
    pub fn new(
        runtime: &Rc<RefCell<RuntimeStandalone>>,
        account_id: AccountId,
        signer: InMemorySigner,
    ) -> Self {
        let runtime = Rc::clone(runtime);
        Self { runtime, account_id, signer }
    }

    pub fn account_id(&self) -> AccountId {
        self.account_id.clone()
    }

    pub fn account(&self) -> Option<Account> {
        (*self.runtime).borrow().view_account(&self.account_id)
    }

    pub fn transfer(&self, to: AccountId, deposit: Balance) -> ExecutionResult {
        self.submit_transaction(self.transaction(to).transfer(deposit))
    }

    pub fn call(
        &self,
        pending_tx: PendingContractTx,
        deposit: Balance,
        gas: Gas,
    ) -> ExecutionResult {
        self.submit_transaction(self.transaction(pending_tx.receiver_id).function_call(
            pending_tx.method.to_string(),
            pending_tx.args,
            gas,
            deposit,
        ))
    }

    pub fn deploy_and_init(
        &self,
        wasm_bytes: &[u8],
        pending_tx: PendingContractTx,
        deposit: Balance,
        gas: Gas,
    ) -> UserAccount {
        let signer = InMemorySigner::from_seed(
            &pending_tx.receiver_id.clone(),
            KeyType::ED25519,
            &pending_tx.receiver_id.clone(),
        );
        let account_id = pending_tx.receiver_id.clone();
        self.submit_transaction(
            self.transaction(pending_tx.receiver_id)
                .create_account()
                .add_key(signer.public_key(), AccessKey::full_access())
                .transfer(deposit)
                .deploy_contract(wasm_bytes.to_vec())
                .function_call(pending_tx.method, pending_tx.args, gas, 0),
        )
        .assert_success();
        UserAccount::new(&self.runtime, account_id, signer)
    }

    pub fn deploy(
        &self,
        wasm_bytes: &[u8],
        account_id: AccountId,
        deposit: Balance,
    ) -> UserAccount {
        let signer =
            InMemorySigner::from_seed(&account_id.clone(), KeyType::ED25519, &account_id.clone());
        self.submit_transaction(
            self.transaction(account_id.clone())
                .create_account()
                .add_key(signer.public_key(), AccessKey::full_access())
                .transfer(deposit)
                .deploy_contract(wasm_bytes.to_vec()),
        )
        .assert_success();
        UserAccount::new(&self.runtime, account_id, signer)
    }

    pub fn transaction(&self, receiver_id: AccountId) -> Transaction {
        let nonce = (*self.runtime)
            .borrow()
            .view_access_key(&self.account_id, &self.signer.public_key())
            .unwrap()
            .nonce
            + 1;
        Transaction::new(
            self.account_id.clone(),
            self.signer.public_key(),
            receiver_id,
            nonce,
            CryptoHash::default(),
        )
    }

    pub fn submit_transaction(&self, transaction: Transaction) -> ExecutionResult {
        let res = (*self.runtime).borrow_mut().resolve_tx(transaction.sign(&self.signer)).unwrap();
        (*self.runtime).borrow_mut().process_all().unwrap();
        outcome_into_result(res, &self.runtime)
    }

    pub fn view(&self, pending_tx: PendingContractTx) -> ViewResult {
        (*self.runtime).borrow().view_method_call(
            &pending_tx.receiver_id,
            &pending_tx.method,
            &pending_tx.args,
        )
    }

    pub fn create_user_from(
        &self,
        signer_user: &UserAccount,
        account_id: AccountId,
        amount: Balance,
    ) -> UserAccount {
        let signer = InMemorySigner::from_seed(&account_id.clone(), KeyType::ED25519, &account_id);
        signer_user
            .submit_transaction(
                signer_user
                    .transaction(account_id.clone())
                    .create_account()
                    .add_key(signer.public_key(), AccessKey::full_access())
                    .transfer(amount),
            )
            .assert_success();
        let account_id = account_id.clone();
        UserAccount { runtime: Rc::clone(&self.runtime), account_id, signer }
    }

    pub fn create_user(&self, account_id: AccountId, amount: Balance) -> UserAccount {
        self.create_user_from(&self, account_id, amount)
    }
}

pub struct ContractAccount<T> {
    pub user_account: UserAccount,
    pub contract: T,
}

pub fn init_simulator(genesis_config: Option<GenesisConfig>) -> UserAccount {
    let (runtime, signer, root_account_id) = init_runtime(genesis_config);
    UserAccount::new(&Rc::new(RefCell::new(runtime)), root_account_id, signer)
}

/// Deploys a contract. Will either deploy or deploy and initialize a contract.
/// Returns a `ContractAccount<T>` where `T` is the first argument.
/// Note: currently init methods are expected to be `new`
///
/// # Examples
///  This example deploys and
/// ```
///     let contract_user = deploy!(
///        // Contract Proxy
///        FungibleTokenContract,
///        // Contract account id
///        "contract",
///        // Referennce to bytes of contract
///        &TOKEN_WASM_BYTES,
///        // User deploying the contract,
///        master_account,
///        
///        // Args to initialize contract
///        master_account.account_id(),
///        initial_balance.into(),
///        
///    );
/// ```
#[macro_export]
macro_rules! deploy {
    ($contract: ident, $account_id:expr, $wasm_bytes: expr, $user:expr, $deposit: expr) => {
        ContractAccount {
            user_account: $user.deploy($wasm_bytes, $account_id.to_string(), $deposit),
            contract: $contract { account_id: $account_id.to_string() },
        }
    };
    ($contract: ident, $account_id:expr, $wasm_bytes: expr, $user_id:expr, $deposit:expr, $gas:expr, $method: ident, $($arg:expr),+ ) => {
           {
               let __contract = $contract { account_id: $account_id.to_string() };
               ContractAccount {
                   user_account: $user_id.deploy_and_init($wasm_bytes, __contract.$method($($arg),+), $deposit, $gas),
                   contract: __contract,
               }
           }
       };
}

#[macro_export]
macro_rules! deploy_default {
    ($contract: ident, $account_id:expr, $wasm_bytes: expr, $user:expr) => {
        ContractAccount {
            user_account: $user.deploy($wasm_bytes, $account_id.to_string(), near_sdk_sim::STORAGE_AMOUNT),
            contract: $contract { account_id: $account_id.to_string() },
        }
    };
    ($contract: ident, $account_id:expr, $wasm_bytes:expr, $user_id:expr, $method:ident, $($arg:expr),+ ) => {
           {
               let __contract = $contract { account_id: $account_id.to_string() };
               ContractAccount {
                   user_account: $user_id.deploy_and_init($wasm_bytes, __contract.$method($($arg),+), near_sdk_sim::STORAGE_AMOUNT, near_sdk_sim::DEFAULT_GAS),
                   contract: __contract,
               }
           }
       };
}
