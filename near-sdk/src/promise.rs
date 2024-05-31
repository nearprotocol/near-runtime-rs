#[cfg(feature = "abi")]
use borsh::BorshSchema;
use std::cell::RefCell;
#[cfg(feature = "abi")]
use std::collections::BTreeMap;
use std::io::{Error, Write};
use std::num::NonZeroU128;
use std::rc::Rc;

use crate::env::migrate_to_allowance;
use crate::{errors, AccountId, Gas, GasWeight, NearToken, PromiseIndex, PublicKey};

/// Allow an access key to spend either an unlimited or limited amount of gas
// This wrapper prevents incorrect construction
#[derive(Clone, Copy)]
pub enum Allowance {
    Unlimited,
    Limited(NonZeroU128),
}

impl Allowance {
    pub fn unlimited() -> Allowance {
        Allowance::Unlimited
    }

    /// This will return an None if you try to pass a zero value balance
    pub fn limited(balance: NearToken) -> Option<Allowance> {
        NonZeroU128::new(balance.as_yoctonear()).map(Allowance::Limited)
    }
}

enum PromiseAction {
    CreateAccount,
    DeployContract {
        code: Vec<u8>,
    },
    FunctionCall {
        function_name: String,
        arguments: Vec<u8>,
        amount: NearToken,
        gas: Gas,
    },
    FunctionCallWeight {
        function_name: String,
        arguments: Vec<u8>,
        amount: NearToken,
        gas: Gas,
        weight: GasWeight,
    },
    Transfer {
        amount: NearToken,
    },
    Stake {
        amount: NearToken,
        public_key: PublicKey,
    },
    AddFullAccessKey {
        public_key: PublicKey,
        nonce: u64,
    },
    AddAccessKey {
        public_key: PublicKey,
        allowance: Allowance,
        receiver_id: AccountId,
        function_names: String,
        nonce: u64,
    },
    DeleteKey {
        public_key: PublicKey,
    },
    DeleteAccount {
        beneficiary_id: AccountId,
    },
}

impl PromiseAction {
    pub fn add(&self, promise_index: PromiseIndex) {
        use PromiseAction::*;
        match self {
            CreateAccount => crate::env::promise_batch_action_create_account(promise_index),
            DeployContract { code } => {
                crate::env::promise_batch_action_deploy_contract(promise_index, code)
            }
            FunctionCall { function_name, arguments, amount, gas } => {
                crate::env::promise_batch_action_function_call(
                    promise_index,
                    function_name,
                    arguments,
                    *amount,
                    *gas,
                )
            }
            FunctionCallWeight { function_name, arguments, amount, gas, weight } => {
                crate::env::promise_batch_action_function_call_weight(
                    promise_index,
                    function_name,
                    arguments,
                    *amount,
                    *gas,
                    GasWeight(weight.0),
                )
            }
            Transfer { amount } => {
                crate::env::promise_batch_action_transfer(promise_index, *amount)
            }
            Stake { amount, public_key } => {
                crate::env::promise_batch_action_stake(promise_index, *amount, public_key)
            }
            AddFullAccessKey { public_key, nonce } => {
                crate::env::promise_batch_action_add_key_with_full_access(
                    promise_index,
                    public_key,
                    *nonce,
                )
            }
            AddAccessKey { public_key, allowance, receiver_id, function_names, nonce } => {
                crate::env::promise_batch_action_add_key_allowance_with_function_call(
                    promise_index,
                    public_key,
                    *nonce,
                    *allowance,
                    receiver_id,
                    function_names,
                )
            }
            DeleteKey { public_key } => {
                crate::env::promise_batch_action_delete_key(promise_index, public_key)
            }
            DeleteAccount { beneficiary_id } => {
                crate::env::promise_batch_action_delete_account(promise_index, beneficiary_id)
            }
        }
    }
}

struct PromiseSingle {
    pub account_id: AccountId,
    pub actions: RefCell<Vec<PromiseAction>>,
    pub after: RefCell<Option<Promise>>,
    /// Promise index that is computed only once.
    pub promise_index: RefCell<Option<PromiseIndex>>,
}

impl PromiseSingle {
    pub fn construct_recursively(&self) -> PromiseIndex {
        let mut promise_lock = self.promise_index.borrow_mut();
        if let Some(res) = promise_lock.as_ref() {
            return *res;
        }
        let promise_index = if let Some(after) = self.after.borrow().as_ref() {
            crate::env::promise_batch_then(after.construct_recursively(), &self.account_id)
        } else {
            crate::env::promise_batch_create(&self.account_id)
        };
        let actions_lock = self.actions.borrow();
        for action in actions_lock.iter() {
            action.add(promise_index);
        }
        *promise_lock = Some(promise_index);
        promise_index
    }
}

pub struct PromiseJoint {
    pub promise_a: Promise,
    pub promise_b: Promise,
    /// Promise index that is computed only once.
    pub promise_index: RefCell<Option<PromiseIndex>>,
}

impl PromiseJoint {
    pub fn construct_recursively(&self) -> PromiseIndex {
        let mut promise_lock = self.promise_index.borrow_mut();
        if let Some(res) = promise_lock.as_ref() {
            return *res;
        }
        let res = crate::env::promise_and(&[
            self.promise_a.construct_recursively(),
            self.promise_b.construct_recursively(),
        ]);
        *promise_lock = Some(res);
        res
    }
}

/// A structure representing a result of the scheduled execution on another contract.
///
/// Smart contract developers will explicitly use `Promise` in two situations:
/// * When they need to return `Promise`.
///
///   In the following code if someone calls method `ContractA::a` they will internally cause an
///   execution of method `ContractB::b` of `bob_near` account, and the return value of `ContractA::a`
///   will be what `ContractB::b` returned.
/// ```no_run
/// # use near_sdk::{ext_contract, near, Promise, Gas};
/// #[ext_contract]
/// pub trait ContractB {
///     fn b(&mut self);
/// }
///
/// #[near(contract_state)]
/// #[derive(Default)]
/// struct ContractA {}
///
/// #[near]
/// impl ContractA {
///     pub fn a(&self) -> Promise {
///         contract_b::ext("bob_near".parse().unwrap()).b()
///     }
/// }
/// ```
///
/// * When they need to create a transaction with one or many actions, e.g. the following code
///   schedules a transaction that creates an account, transfers tokens, and assigns a public key:
///
/// ```no_run
/// # use near_sdk::{Promise, env, test_utils::VMContextBuilder, testing_env, Gas, NearToken};
/// # testing_env!(VMContextBuilder::new().signer_account_id("bob_near".parse().unwrap())
/// #               .account_balance(NearToken::from_yoctonear(1000)).prepaid_gas(Gas::from_gas(1_000_000)).build());
/// Promise::new("bob_near".parse().unwrap())
///   .create_account()
///   .transfer(NearToken::from_yoctonear(1000))
///   .add_full_access_key(env::signer_account_pk());
/// ```
pub struct Promise {
    subtype: PromiseSubtype,
    should_return: RefCell<bool>,
}

/// Until we implement strongly typed promises we serialize them as unit struct.
#[cfg(feature = "abi")]
impl BorshSchema for Promise {
    fn add_definitions_recursively(
        definitions: &mut BTreeMap<borsh::schema::Declaration, borsh::schema::Definition>,
    ) {
        <()>::add_definitions_recursively(definitions);
    }

    fn declaration() -> borsh::schema::Declaration {
        <()>::declaration()
    }
}

#[derive(Clone)]
enum PromiseSubtype {
    Single(Rc<PromiseSingle>),
    Joint(Rc<PromiseJoint>),
}

impl Promise {
    /// Create a promise that acts on the given account.
    pub fn new(account_id: AccountId) -> Self {
        Self {
            subtype: PromiseSubtype::Single(Rc::new(PromiseSingle {
                account_id,
                actions: RefCell::new(vec![]),
                after: RefCell::new(None),
                promise_index: RefCell::new(None),
            })),
            should_return: RefCell::new(false),
        }
    }

    fn add_action(self, action: PromiseAction) -> Self {
        match &self.subtype {
            PromiseSubtype::Single(x) => x.actions.borrow_mut().push(action),
            PromiseSubtype::Joint(_) => {
                crate::env::panic_err(errors::ActionInJointPromise::new().into())
            }
        }
        self
    }

    /// Create account on which this promise acts.
    pub fn create_account(self) -> Self {
        self.add_action(PromiseAction::CreateAccount)
    }

    /// Deploy a smart contract to the account on which this promise acts.
    pub fn deploy_contract(self, code: Vec<u8>) -> Self {
        self.add_action(PromiseAction::DeployContract { code })
    }

    /// A low-level interface for making a function call to the account that this promise acts on.
    pub fn function_call(
        self,
        function_name: String,
        arguments: Vec<u8>,
        amount: NearToken,
        gas: Gas,
    ) -> Self {
        self.add_action(PromiseAction::FunctionCall { function_name, arguments, amount, gas })
    }

    /// A low-level interface for making a function call to the account that this promise acts on.
    /// unlike [`Promise::function_call`], this function accepts a weight to use relative unused gas
    /// on this function call at the end of the scheduling method execution.
    pub fn function_call_weight(
        self,
        function_name: String,
        arguments: Vec<u8>,
        amount: NearToken,
        gas: Gas,
        weight: GasWeight,
    ) -> Self {
        self.add_action(PromiseAction::FunctionCallWeight {
            function_name,
            arguments,
            amount,
            gas,
            weight,
        })
    }

    /// Transfer tokens to the account that this promise acts on.
    pub fn transfer(self, amount: NearToken) -> Self {
        self.add_action(PromiseAction::Transfer { amount })
    }

    /// Stake the account for the given amount of tokens using the given public key.
    pub fn stake(self, amount: NearToken, public_key: PublicKey) -> Self {
        self.add_action(PromiseAction::Stake { amount, public_key })
    }

    /// Add full access key to the given account.
    pub fn add_full_access_key(self, public_key: PublicKey) -> Self {
        self.add_full_access_key_with_nonce(public_key, 0)
    }

    /// Add full access key to the given account with a provided nonce.
    pub fn add_full_access_key_with_nonce(self, public_key: PublicKey, nonce: u64) -> Self {
        self.add_action(PromiseAction::AddFullAccessKey { public_key, nonce })
    }

    /// Add an access key that is restricted to only calling a smart contract on some account using
    /// only a restricted set of methods. Here `function_names` is a comma separated list of methods,
    /// e.g. `"method_a,method_b".to_string()`.
    pub fn add_access_key_allowance(
        self,
        public_key: PublicKey,
        allowance: Allowance,
        receiver_id: AccountId,
        function_names: String,
    ) -> Self {
        self.add_access_key_allowance_with_nonce(
            public_key,
            allowance,
            receiver_id,
            function_names,
            0,
        )
    }

    #[deprecated(since = "5.0.0", note = "Use add_access_key_allowance instead")]
    pub fn add_access_key(
        self,
        public_key: PublicKey,
        allowance: NearToken,
        receiver_id: AccountId,
        function_names: String,
    ) -> Self {
        let allowance = migrate_to_allowance(allowance);
        self.add_access_key_allowance(public_key, allowance, receiver_id, function_names)
    }

    /// Add an access key with a provided nonce.
    pub fn add_access_key_allowance_with_nonce(
        self,
        public_key: PublicKey,
        allowance: Allowance,
        receiver_id: AccountId,
        function_names: String,
        nonce: u64,
    ) -> Self {
        self.add_action(PromiseAction::AddAccessKey {
            public_key,
            allowance,
            receiver_id,
            function_names,
            nonce,
        })
    }

    #[deprecated(since = "5.0.0", note = "Use add_access_key_allowance_with_nonce instead")]
    pub fn add_access_key_with_nonce(
        self,
        public_key: PublicKey,
        allowance: NearToken,
        receiver_id: AccountId,
        function_names: String,
        nonce: u64,
    ) -> Self {
        let allowance = migrate_to_allowance(allowance);
        self.add_access_key_allowance_with_nonce(
            public_key,
            allowance,
            receiver_id,
            function_names,
            nonce,
        )
    }

    /// Delete access key from the given account.
    pub fn delete_key(self, public_key: PublicKey) -> Self {
        self.add_action(PromiseAction::DeleteKey { public_key })
    }

    /// Delete the given account.
    pub fn delete_account(self, beneficiary_id: AccountId) -> Self {
        self.add_action(PromiseAction::DeleteAccount { beneficiary_id })
    }

    /// Merge this promise with another promise, so that we can schedule execution of another
    /// smart contract right after all merged promises finish.
    ///
    /// Note, once the promises are merged it is not possible to add actions to them, e.g. the
    /// following code will panic during the execution of the smart contract:
    ///
    /// ```no_run
    /// # use near_sdk::{Promise, testing_env};
    /// let p1 = Promise::new("bob_near".parse().unwrap()).create_account();
    /// let p2 = Promise::new("carol_near".parse().unwrap()).create_account();
    /// let p3 = p1.and(p2);
    /// // p3.create_account();
    /// ```
    pub fn and(self, other: Promise) -> Promise {
        Promise {
            subtype: PromiseSubtype::Joint(Rc::new(PromiseJoint {
                promise_a: self,
                promise_b: other,
                promise_index: RefCell::new(None),
            })),
            should_return: RefCell::new(false),
        }
    }

    /// Schedules execution of another promise right after the current promise finish executing.
    ///
    /// In the following code `bob_near` and `dave_near` will be created concurrently. `carol_near`
    /// creation will wait for `bob_near` to be created, and `eva_near` will wait for both `carol_near`
    /// and `dave_near` to be created first.
    /// ```no_run
    /// # use near_sdk::{Promise, VMContext, testing_env};
    /// let p1 = Promise::new("bob_near".parse().unwrap()).create_account();
    /// let p2 = Promise::new("carol_near".parse().unwrap()).create_account();
    /// let p3 = Promise::new("dave_near".parse().unwrap()).create_account();
    /// let p4 = Promise::new("eva_near".parse().unwrap()).create_account();
    /// p1.then(p2).and(p3).then(p4);
    /// ```
    pub fn then(self, mut other: Promise) -> Promise {
        match &mut other.subtype {
            PromiseSubtype::Single(x) => {
                let mut after = x.after.borrow_mut();
                if after.is_some() {
                    crate::env::panic_err(errors::PromiseAlreadyScheduled::new().into());
                }
                *after = Some(self)
            }
            PromiseSubtype::Joint(_) => {
                crate::env::panic_err(errors::CallbackJointPromise::new().into())
            }
        }
        other
    }

    /// A specialized, relatively low-level API method. Allows to mark the given promise as the one
    /// that should be considered as a return value.
    ///
    /// In the below code `a1` and `a2` functions are equivalent.
    /// ```
    /// # use near_sdk::{ext_contract, Gas, near, Promise};
    /// #[ext_contract]
    /// pub trait ContractB {
    ///     fn b(&mut self);
    /// }
    ///
    /// #[near(contract_state)]
    /// #[derive(Default)]
    /// struct ContractA {}
    ///
    /// #[near]
    /// impl ContractA {
    ///     pub fn a1(&self) {
    ///        contract_b::ext("bob_near".parse().unwrap()).b().as_return();
    ///     }
    ///
    ///     pub fn a2(&self) -> Promise {
    ///        contract_b::ext("bob_near".parse().unwrap()).b()
    ///     }
    /// }
    /// ```
    #[allow(clippy::wrong_self_convention)]
    pub fn as_return(self) -> Self {
        *self.should_return.borrow_mut() = true;
        self
    }

    fn construct_recursively(&self) -> PromiseIndex {
        let res = match &self.subtype {
            PromiseSubtype::Single(x) => x.construct_recursively(),
            PromiseSubtype::Joint(x) => x.construct_recursively(),
        };
        if *self.should_return.borrow() {
            crate::env::promise_return(res);
        }
        res
    }
}

impl Drop for Promise {
    fn drop(&mut self) {
        self.construct_recursively();
    }
}

impl serde::Serialize for Promise {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        *self.should_return.borrow_mut() = true;
        serializer.serialize_unit()
    }
}

impl borsh::BorshSerialize for Promise {
    fn serialize<W: Write>(&self, _writer: &mut W) -> Result<(), Error> {
        *self.should_return.borrow_mut() = true;

        // Intentionally no bytes written for the promise, the return value from the promise
        // will be considered as the return value from the contract call.
        Ok(())
    }
}

#[cfg(feature = "abi")]
impl schemars::JsonSchema for Promise {
    fn schema_name() -> String {
        "Promise".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        // Since promises are untyped, for now we represent Promise results with the schema
        // `true` which matches everything (i.e. always passes validation)
        schemars::schema::Schema::Bool(true)
    }
}

/// When the method can return either a promise or a value, it can be called with `PromiseOrValue::Promise`
/// or `PromiseOrValue::Value` to specify which one should be returned.
/// # Example
/// ```no_run
/// # use near_sdk::{ext_contract, near, Gas, PromiseOrValue};
/// #[ext_contract]
/// pub trait ContractA {
///     fn a(&mut self);
/// }
///
/// let value = Some(true);
/// let val: PromiseOrValue<bool> = if let Some(value) = value {
///     PromiseOrValue::Value(value)
/// } else {
///     contract_a::ext("bob_near".parse().unwrap()).a().into()
/// };
/// ```
#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum PromiseOrValue<T> {
    Promise(Promise),
    Value(T),
}

#[cfg(feature = "abi")]
impl<T> BorshSchema for PromiseOrValue<T>
where
    T: BorshSchema,
{
    fn add_definitions_recursively(
        definitions: &mut BTreeMap<borsh::schema::Declaration, borsh::schema::Definition>,
    ) {
        T::add_definitions_recursively(definitions);
    }

    fn declaration() -> borsh::schema::Declaration {
        T::declaration()
    }
}

impl<T> From<Promise> for PromiseOrValue<T> {
    fn from(promise: Promise) -> Self {
        PromiseOrValue::Promise(promise)
    }
}

impl<T: borsh::BorshSerialize> borsh::BorshSerialize for PromiseOrValue<T> {
    fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        match self {
            // Only actual value is serialized.
            PromiseOrValue::Value(x) => x.serialize(writer),
            // The promise is dropped to cause env::promise calls.
            PromiseOrValue::Promise(p) => p.serialize(writer),
        }
    }
}

#[cfg(feature = "abi")]
impl<T: schemars::JsonSchema> schemars::JsonSchema for PromiseOrValue<T> {
    fn schema_name() -> String {
        format!("PromiseOrValue{}", T::schema_name())
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        T::json_schema(gen)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use crate::mock::MockAction;
    use crate::test_utils::get_created_receipts;
    use crate::test_utils::test_env::{alice, bob};
    use crate::{
        test_utils::VMContextBuilder, testing_env, AccountId, Allowance, NearToken, Promise,
        PublicKey,
    };

    fn pk() -> PublicKey {
        "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp".parse().unwrap()
    }

    fn get_actions() -> std::vec::IntoIter<MockAction> {
        let receipts = get_created_receipts();
        let first_receipt = receipts.into_iter().next().unwrap();
        first_receipt.actions.into_iter()
    }

    fn has_add_key_with_full_access(public_key: PublicKey, nonce: Option<u64>) -> bool {
        let public_key = near_crypto::PublicKey::try_from(public_key).unwrap();
        get_actions().any(|el| {
            matches!(
                el,
                MockAction::AddKeyWithFullAccess { public_key: p, nonce: n, receipt_index: _, }
                if p == public_key
                    && (nonce.is_none() || Some(n) == nonce)
            )
        })
    }

    fn has_add_key_with_function_call(
        public_key: PublicKey,
        allowance: u128,
        receiver_id: AccountId,
        function_names: String,
        nonce: Option<u64>,
    ) -> bool {
        let public_key = near_crypto::PublicKey::try_from(public_key).unwrap();
        get_actions().any(|el| {
            matches!(
                el,
                MockAction::AddKeyWithFunctionCall {
                    public_key: p,
                    allowance: a,
                    receiver_id: r,
                    method_names,
                    nonce: n,
                    receipt_index: _,
                }
                if p == public_key
                    && a.unwrap() == NearToken::from_yoctonear(allowance)
                    && r == receiver_id
                    && method_names.clone() == function_names.split(',').collect::<Vec<_>>()
                    && (nonce.is_none() || Some(n) == nonce)
            )
        })
    }

    #[test]
    fn test_add_full_access_key() {
        testing_env!(VMContextBuilder::new().signer_account_id(alice()).build());

        let public_key: PublicKey = pk();

        // Promise is only executed when dropped so we put it in its own scope to make sure receipts
        // are ready afterwards.
        {
            Promise::new(alice()).create_account().add_full_access_key(public_key.clone());
        }

        assert!(has_add_key_with_full_access(public_key, None));
    }

    #[test]
    fn test_add_full_access_key_with_nonce() {
        testing_env!(VMContextBuilder::new().signer_account_id(alice()).build());

        let public_key: PublicKey = pk();
        let nonce = 42;

        {
            Promise::new(alice())
                .create_account()
                .add_full_access_key_with_nonce(public_key.clone(), nonce);
        }

        assert!(has_add_key_with_full_access(public_key, Some(nonce)));
    }

    #[test]
    fn test_add_access_key_allowance() {
        testing_env!(VMContextBuilder::new().signer_account_id(alice()).build());

        let public_key: PublicKey = pk();
        let allowance = 100;
        let receiver_id = bob();
        let function_names = "method_a,method_b".to_string();

        {
            Promise::new(alice()).create_account().add_access_key_allowance(
                public_key.clone(),
                Allowance::Limited(allowance.try_into().unwrap()),
                receiver_id.clone(),
                function_names.clone(),
            );
        }

        assert!(has_add_key_with_function_call(
            public_key,
            allowance,
            receiver_id,
            function_names,
            None
        ));
    }

    #[test]
    fn test_add_access_key() {
        testing_env!(VMContextBuilder::new().signer_account_id(alice()).build());

        let public_key: PublicKey = pk();
        let allowance = NearToken::from_yoctonear(100);
        let receiver_id = bob();
        let function_names = "method_a,method_b".to_string();

        {
            #[allow(deprecated)]
            Promise::new(alice()).create_account().add_access_key(
                public_key.clone(),
                allowance,
                receiver_id.clone(),
                function_names.clone(),
            );
        }

        assert!(has_add_key_with_function_call(
            public_key,
            allowance.as_yoctonear(),
            receiver_id,
            function_names,
            None
        ));
    }

    #[test]
    fn test_add_access_key_allowance_with_nonce() {
        testing_env!(VMContextBuilder::new().signer_account_id(alice()).build());

        let public_key: PublicKey = pk();
        let allowance = 100;
        let receiver_id = bob();
        let function_names = "method_a,method_b".to_string();
        let nonce = 42;

        {
            Promise::new(alice()).create_account().add_access_key_allowance_with_nonce(
                public_key.clone(),
                Allowance::Limited(allowance.try_into().unwrap()),
                receiver_id.clone(),
                function_names.clone(),
                nonce,
            );
        }

        assert!(has_add_key_with_function_call(
            public_key,
            allowance,
            receiver_id,
            function_names,
            Some(nonce)
        ));
    }

    #[test]
    fn test_add_access_key_with_nonce() {
        testing_env!(VMContextBuilder::new().signer_account_id(alice()).build());

        let public_key: PublicKey = pk();
        let allowance = NearToken::from_yoctonear(100);
        let receiver_id = bob();
        let function_names = "method_a,method_b".to_string();
        let nonce = 42;

        {
            #[allow(deprecated)]
            Promise::new(alice()).create_account().add_access_key_with_nonce(
                public_key.clone(),
                allowance,
                receiver_id.clone(),
                function_names.clone(),
                nonce,
            );
        }

        assert!(has_add_key_with_function_call(
            public_key,
            allowance.as_yoctonear(),
            receiver_id,
            function_names,
            Some(nonce)
        ));
    }

    #[test]
    fn test_delete_key() {
        testing_env!(VMContextBuilder::new().signer_account_id(alice()).build());

        let public_key: PublicKey = pk();

        {
            Promise::new(alice())
                .create_account()
                .add_full_access_key(public_key.clone())
                .delete_key(public_key.clone());
        }
        let public_key = near_crypto::PublicKey::try_from(public_key).unwrap();

        let has_action = get_actions().any(|el| {
            matches!(
                el,
                MockAction::DeleteKey { public_key: p , receipt_index: _, } if p == public_key
            )
        });
        assert!(has_action);
    }
}
