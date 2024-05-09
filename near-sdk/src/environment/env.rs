//! Blockchain-specific methods available to the smart contract. This is a wrapper around a
//! low-level `BlockchainInterface`. Unless you know what you are doing prefer using `env::*`
//! whenever possible. In case of cross-contract calls prefer using even higher-level API available
//! through `callback_args`, `callback_args_vec`, `ext_contract`, `Promise`, and `PromiseOrValue`.

use std::convert::TryInto;
use std::mem::{size_of, size_of_val};
use std::panic as std_panic;
use std::{convert::TryFrom, mem::MaybeUninit};

#[cfg(all(not(target_arch = "wasm32"), feature = "unit-testing"))]
use crate::mock::MockedBlockchain;
use crate::promise::Allowance;
use crate::types::{
    AccountId, BlockHeight, Gas, NearToken, PromiseIndex, PromiseResult, PublicKey, StorageUsage,
};
use crate::{CryptoHash, GasWeight, PromiseError};
use near_sys as sys;

const REGISTER_EXPECTED_ERR: &str =
    "Register was expected to have data because we just wrote it into it.";

/// Register used internally for atomic operations. This register is safe to use by the user,
/// since it only needs to be untouched while methods of `Environment` execute, which is guaranteed
/// guest code is not parallel.
const ATOMIC_OP_REGISTER: u64 = std::u64::MAX - 2;
/// Register used to record evicted values from the storage.
const EVICTED_REGISTER: u64 = std::u64::MAX - 1;

/// Key used to store the state of the contract.
const STATE_KEY: &[u8] = b"STATE";

/// The minimum length of a valid account ID.
const MIN_ACCOUNT_ID_LEN: u64 = 2;
/// The maximum length of a valid account ID.
const MAX_ACCOUNT_ID_LEN: u64 = 64;

fn expect_register<T>(option: Option<T>) -> T {
    option.unwrap_or_else(|| panic_str(REGISTER_EXPECTED_ERR))
}

/// A simple macro helper to read blob value coming from host's method.
macro_rules! try_method_into_register {
    ( $method:ident ) => {{
        unsafe { sys::$method(ATOMIC_OP_REGISTER) };
        read_register(ATOMIC_OP_REGISTER)
    }};
}

/// Same as `try_method_into_register` but expects the data.
macro_rules! method_into_register {
    ( $method:ident ) => {{
        expect_register(try_method_into_register!($method))
    }};
}

//* Note: need specific length functions because const generics don't work with mem::transmute
//* https://github.com/rust-lang/rust/issues/61956

pub(crate) unsafe fn read_register_fixed_20(register_id: u64) -> [u8; 20] {
    let mut hash = [MaybeUninit::<u8>::uninit(); 20];
    sys::read_register(register_id, hash.as_mut_ptr() as _);
    std::mem::transmute(hash)
}

pub(crate) unsafe fn read_register_fixed_32(register_id: u64) -> [u8; 32] {
    let mut hash = [MaybeUninit::<u8>::uninit(); 32];
    sys::read_register(register_id, hash.as_mut_ptr() as _);
    std::mem::transmute(hash)
}

pub(crate) unsafe fn read_register_fixed_64(register_id: u64) -> [u8; 64] {
    let mut hash = [MaybeUninit::<u8>::uninit(); 64];
    sys::read_register(register_id, hash.as_mut_ptr() as _);
    std::mem::transmute(hash)
}

/// Replaces the current low-level blockchain interface accessible through `env::*` with another
/// low-level blockchain interfacr that implements `BlockchainInterface` trait. In most cases you
/// want to use `testing_env!` macro to set it.
///
/// ```
/// # let context = near_sdk::test_utils::VMContextBuilder::new().build();
/// # let vm_config = near_sdk::test_vm_config();
/// # let fees_config = near_sdk::RuntimeFeesConfig::test();
/// # let storage = Default::default();
/// # let validators = Default::default();
/// let mocked_blockchain = near_sdk::MockedBlockchain::new(
///           context,
///           vm_config,
///           fees_config,
///           vec![],
///           storage,
///           validators,
///           None,
///       );
/// near_sdk::env::set_blockchain_interface(mocked_blockchain);
/// ```
#[cfg(all(not(target_arch = "wasm32"), feature = "unit-testing"))]
pub fn set_blockchain_interface(blockchain_interface: MockedBlockchain) {
    crate::mock::with_mocked_blockchain(|b| {
        *b = blockchain_interface;
    })
}

/// Implements panic hook that converts `PanicInfo` into a string and provides it through the
/// blockchain interface.
fn panic_hook_impl(info: &std_panic::PanicInfo) {
    panic_str(info.to_string().as_str());
}

/// Setups panic hook to expose error info to the blockchain.
pub fn setup_panic_hook() {
    std_panic::set_hook(Box::new(panic_hook_impl));
}

/// Reads the content of the `register_id`. If register is not used returns `None`.
pub fn read_register(register_id: u64) -> Option<Vec<u8>> {
    // Get register length and convert to a usize. The max register size in config is much less
    // than the u32 max so the abort should never be hit, but is there for safety because there
    // would be undefined behaviour during `read_register` if the buffer length is truncated.
    let len: usize = register_len(register_id)?.try_into().unwrap_or_else(|_| abort());

    // Initialize buffer with capacity.
    let mut buffer = Vec::with_capacity(len);

    // Read register into buffer.
    //* SAFETY: This is safe because the buffer is initialized with the exact capacity of the
    //*         register that is being read from.
    unsafe {
        sys::read_register(register_id, buffer.as_mut_ptr() as u64);

        // Set updated length after writing to buffer.
        buffer.set_len(len);
    }
    Some(buffer)
}

/// Returns the size of the register. If register is not used returns `None`.
pub fn register_len(register_id: u64) -> Option<u64> {
    let len = unsafe { sys::register_len(register_id) };
    if len == std::u64::MAX {
        None
    } else {
        Some(len)
    }
}

// ###############
// # Context API #
// ###############
/// The id of the account that owns the current contract.
pub fn current_account_id() -> AccountId {
    assert_valid_account_id(method_into_register!(current_account_id))
}

/// The id of the account that either signed the original transaction or issued the initial
/// cross-contract call.
pub fn signer_account_id() -> AccountId {
    assert_valid_account_id(method_into_register!(signer_account_id))
}

/// The public key of the account that did the signing.
pub fn signer_account_pk() -> PublicKey {
    PublicKey::try_from(method_into_register!(signer_account_pk)).unwrap_or_else(|_| abort())
}

/// The id of the account that was the previous contract in the chain of cross-contract calls.
/// If this is the first contract, it is equal to `signer_account_id`.
pub fn predecessor_account_id() -> AccountId {
    assert_valid_account_id(method_into_register!(predecessor_account_id))
}

/// Helper function to convert and check the account ID from bytes from the runtime.
fn assert_valid_account_id(bytes: Vec<u8>) -> AccountId {
    String::from_utf8(bytes)
        .ok()
        .and_then(|s| AccountId::try_from(s).ok())
        .unwrap_or_else(|| abort())
}

/// The input to the contract call serialized as bytes. If input is not provided returns `None`.
pub fn input() -> Option<Vec<u8>> {
    try_method_into_register!(input)
}

/// Current block index.
#[deprecated(since = "4.0.0", note = "Use block_height instead")]
pub fn block_index() -> BlockHeight {
    block_height()
}

/// Returns the height of the block the transaction is being executed in.
pub fn block_height() -> BlockHeight {
    unsafe { sys::block_height() }
}

/// Current block timestamp, i.e, number of non-leap-nanoseconds since January 1, 1970 0:00:00 UTC.
pub fn block_timestamp() -> u64 {
    unsafe { sys::block_timestamp() }
}

/// Current block timestamp, i.e, number of non-leap-milliseconds since January 1, 1970 0:00:00 UTC.
pub fn block_timestamp_ms() -> u64 {
    block_timestamp() / 1_000_000
}

/// Current epoch height.
pub fn epoch_height() -> u64 {
    unsafe { sys::epoch_height() }
}

/// Current total storage usage of this smart contract that this account would be paying for.
pub fn storage_usage() -> StorageUsage {
    unsafe { sys::storage_usage() }
}

// #################
// # Economics API #
// #################
/// The balance attached to the given account. This includes the attached_deposit that was
/// attached to the transaction
pub fn account_balance() -> NearToken {
    let data = [0u8; size_of::<NearToken>()];
    unsafe { sys::account_balance(data.as_ptr() as u64) };
    NearToken::from_yoctonear(u128::from_le_bytes(data))
}

/// The balance locked for potential validator staking.
pub fn account_locked_balance() -> NearToken {
    let data = [0u8; size_of::<NearToken>()];
    unsafe { sys::account_locked_balance(data.as_ptr() as u64) };
    NearToken::from_yoctonear(u128::from_le_bytes(data))
}

/// The balance that was attached to the call that will be immediately deposited before the
/// contract execution starts
pub fn attached_deposit() -> NearToken {
    let data = [0u8; size_of::<NearToken>()];
    unsafe { sys::attached_deposit(data.as_ptr() as u64) };
    NearToken::from_yoctonear(u128::from_le_bytes(data))
}

/// The amount of gas attached to the call that can be used to pay for the gas fees.
pub fn prepaid_gas() -> Gas {
    Gas::from_gas(unsafe { sys::prepaid_gas() })
}

/// The gas that was already burnt during the contract execution (cannot exceed `prepaid_gas`)
pub fn used_gas() -> Gas {
    Gas::from_gas(unsafe { sys::used_gas() })
}

// ############
// # Math API #
// ############

/// Returns the random seed from the current block. This 32 byte hash is based on the VRF value from
/// the block. This value is not modified in any way each time this function is called within the
/// same method/block.
pub fn random_seed() -> Vec<u8> {
    random_seed_array().to_vec()
}

/// Returns the random seed from the current block. This 32 byte hash is based on the VRF value from
/// the block. This value is not modified in any way each time this function is called within the
/// same method/block.
/// Example of usage:
/// ```rust
/// use rand::{Rng, SeedableRng};
/// use rand_chacha::ChaCha20Rng;
/// use near_sdk::near;
/// use near_sdk::env;
/// #[near(contract_state)]
/// struct RngExample {
///    val: i32,
/// }
/// #[near]
/// impl RngExample {
///     pub fn increment(&mut self) {
///         let mut rng = ChaCha20Rng::from_seed(env::random_seed_array());
///         let value = rng.gen_range(0..1011);
///         self.val += value;
///     }
///     pub fn get_value(&mut self) -> i32 {
///         self.val
///     }
/// }
/// ```
///
/// Example of usage with [near-rng](https://lib.rs/crates/near-rng) which allows to decrease size of contract binary:
///
/// ```rust
/// use near_rng::Rng;
/// use near_sdk::near;
/// use near_sdk::env;
/// #[near(contract_state)]
/// struct NearRngExample {
///    val: i32,
/// }
/// #[near]
/// impl NearRngExample {
///     pub fn increment(&mut self) {
///         let mut rng = Rng::new(&env::random_seed());
///         let value = rng.rand_range_i32(0, 20);
///         self.val += value;
///     }
///     pub fn get_value(&mut self) -> i32 {
///         self.val
///     }
/// }
/// ```
/// More info in [documentation](https://docs.near.org/develop/contracts/security/random)
pub fn random_seed_array() -> [u8; 32] {
    //* SAFETY: random_seed syscall will always generate 32 bytes inside of the atomic op register
    //*         so the read will have a sufficient buffer of 32, and can transmute from uninit
    //*         because all bytes are filled. This assumes a valid random_seed implementation.
    unsafe {
        sys::random_seed(ATOMIC_OP_REGISTER);
        read_register_fixed_32(ATOMIC_OP_REGISTER)
    }
}

/// Hashes the random sequence of bytes using sha256.
///
/// # Examples
///
/// ```
/// use near_sdk::env::sha256;
/// use hex;
///
/// assert_eq!(
///     sha256(b"The phrase that will be hashed"),
///     hex::decode("7fc38bc74a0d0e592d2b8381839adc2649007d5bca11f92eeddef78681b4e3a3").expect("Decoding failed")
/// );
/// ```
pub fn sha256(value: &[u8]) -> Vec<u8> {
    sha256_array(value).to_vec()
}

/// Hashes the random sequence of bytes using keccak256.
pub fn keccak256(value: &[u8]) -> Vec<u8> {
    keccak256_array(value).to_vec()
}

/// Hashes the random sequence of bytes using keccak512.
pub fn keccak512(value: &[u8]) -> Vec<u8> {
    keccak512_array(value).to_vec()
}

/// Hashes the bytes using the SHA-256 hash function. This returns a 32 byte hash.
///
/// # Examples
///
/// ```
/// use near_sdk::env::sha256_array;
/// use hex;
///
/// assert_eq!(
///     &sha256_array(b"The phrase that will be hashed"),
///     hex::decode("7fc38bc74a0d0e592d2b8381839adc2649007d5bca11f92eeddef78681b4e3a3")
///         .expect("Decoding failed")
///         .as_slice()
/// );
/// ```
pub fn sha256_array(value: &[u8]) -> [u8; 32] {
    //* SAFETY: sha256 syscall will always generate 32 bytes inside of the atomic op register
    //*         so the read will have a sufficient buffer of 32, and can transmute from uninit
    //*         because all bytes are filled. This assumes a valid sha256 implementation.
    unsafe {
        sys::sha256(value.len() as _, value.as_ptr() as _, ATOMIC_OP_REGISTER);
        read_register_fixed_32(ATOMIC_OP_REGISTER)
    }
}

/// Hashes the bytes using the Keccak-256 hash function. This returns a 32 byte hash.
pub fn keccak256_array(value: &[u8]) -> [u8; 32] {
    //* SAFETY: keccak256 syscall will always generate 32 bytes inside of the atomic op register
    //*         so the read will have a sufficient buffer of 32, and can transmute from uninit
    //*         because all bytes are filled. This assumes a valid keccak256 implementation.
    unsafe {
        sys::keccak256(value.len() as _, value.as_ptr() as _, ATOMIC_OP_REGISTER);
        read_register_fixed_32(ATOMIC_OP_REGISTER)
    }
}

/// Hashes the bytes using the Keccak-512 hash function. This returns a 64 byte hash.
pub fn keccak512_array(value: &[u8]) -> [u8; 64] {
    //* SAFETY: keccak512 syscall will always generate 64 bytes inside of the atomic op register
    //*         so the read will have a sufficient buffer of 64, and can transmute from uninit
    //*         because all bytes are filled. This assumes a valid keccak512 implementation.
    unsafe {
        sys::keccak512(value.len() as _, value.as_ptr() as _, ATOMIC_OP_REGISTER);
        read_register_fixed_64(ATOMIC_OP_REGISTER)
    }
}

/// Hashes the bytes using the RIPEMD-160 hash function. This returns a 20 byte hash.
pub fn ripemd160_array(value: &[u8]) -> [u8; 20] {
    //* SAFETY: ripemd160 syscall will always generate 20 bytes inside of the atomic op register
    //*         so the read will have a sufficient buffer of 20, and can transmute from uninit
    //*         because all bytes are filled. This assumes a valid ripemd160 implementation.
    unsafe {
        sys::ripemd160(value.len() as _, value.as_ptr() as _, ATOMIC_OP_REGISTER);
        read_register_fixed_20(ATOMIC_OP_REGISTER)
    }
}

/// Recovers an ECDSA signer address from a 32-byte message `hash` and a corresponding `signature`
/// along with `v` recovery byte.
///
/// Takes in an additional flag to check for malleability of the signature
/// which is generally only ideal for transactions.
///
/// Returns 64 bytes representing the public key if the recovery was successful.
#[cfg(feature = "unstable")]
pub fn ecrecover(
    hash: &[u8],
    signature: &[u8],
    v: u8,
    malleability_flag: bool,
) -> Option<[u8; 64]> {
    unsafe {
        let return_code = sys::ecrecover(
            hash.len() as _,
            hash.as_ptr() as _,
            signature.len() as _,
            signature.as_ptr() as _,
            v as u64,
            malleability_flag as u64,
            ATOMIC_OP_REGISTER,
        );
        if return_code == 0 {
            None
        } else {
            Some(read_register_fixed_64(ATOMIC_OP_REGISTER))
        }
    }
}

/// Verifies signature of message using provided ED25519 Public Key
pub fn ed25519_verify(signature: &[u8; 64], message: &[u8], public_key: &[u8; 32]) -> bool {
    unsafe {
        sys::ed25519_verify(
            signature.len() as _,
            signature.as_ptr() as _,
            message.len() as _,
            message.as_ptr() as _,
            public_key.len() as _,
            public_key.as_ptr() as _,
        ) == 1
    }
}

/// Compute alt_bn128 g1 multiexp.
///
/// `alt_bn128` is a specific curve from the Barreto-Naehrig(BN) family. It is particularly
/// well-suited for ZK proofs.
///
/// See also: [EIP-196](https://eips.ethereum.org/EIPS/eip-196)
pub fn alt_bn128_g1_multiexp(value: &[u8]) -> Vec<u8> {
    unsafe {
        sys::alt_bn128_g1_multiexp(value.len() as _, value.as_ptr() as _, ATOMIC_OP_REGISTER);
    };
    read_register(ATOMIC_OP_REGISTER).expect(REGISTER_EXPECTED_ERR)
}

/// Compute alt_bn128 g1 sum.
///
/// `alt_bn128` is a specific curve from the Barreto-Naehrig(BN) family. It is particularly
/// well-suited for ZK proofs.
///
/// See also: [EIP-196](https://eips.ethereum.org/EIPS/eip-196)
pub fn alt_bn128_g1_sum(value: &[u8]) -> Vec<u8> {
    unsafe {
        sys::alt_bn128_g1_sum(value.len() as _, value.as_ptr() as _, ATOMIC_OP_REGISTER);
    };
    read_register(ATOMIC_OP_REGISTER).expect(REGISTER_EXPECTED_ERR)
}

/// Compute pairing check
///
/// `alt_bn128` is a specific curve from the Barreto-Naehrig(BN) family. It is particularly
/// well-suited for ZK proofs.
///
/// See also: [EIP-197](https://eips.ethereum.org/EIPS/eip-197)
pub fn alt_bn128_pairing_check(value: &[u8]) -> bool {
    unsafe { sys::alt_bn128_pairing_check(value.len() as _, value.as_ptr() as _) == 1 }
}

// ################
// # Promises API #
// ################
pub fn promise_yield_create(
    function_name: &str,
    arguments: &[u8],
    gas: Gas,
    weight: GasWeight,
    register_id: u64,
) -> PromiseIndex {
    unsafe {
        PromiseIndex(sys::promise_yield_create(
            function_name.len() as _,
            function_name.as_ptr() as _,
            arguments.len() as _,
            arguments.as_ptr() as _,
            gas.as_gas(),
            weight.0,
            register_id as _,
        ))
    }
}

// TODO: return some kind of success/failure result
pub fn promise_yield_resume(data_id: &CryptoHash, data: &[u8]) -> bool {
    unsafe {
        sys::promise_yield_resume(
            data_id.len() as _,
            data_id.as_ptr() as _,
            data.len() as _,
            data.as_ptr() as _,
        ) != 0
    }
}

/// Creates a promise that will execute a method on account with given arguments and attaches
/// the given amount and gas.
pub fn promise_create(
    account_id: AccountId,
    function_name: &str,
    arguments: &[u8],
    amount: NearToken,
    gas: Gas,
) -> PromiseIndex {
    let account_id = account_id.as_bytes();
    unsafe {
        PromiseIndex(sys::promise_create(
            account_id.len() as _,
            account_id.as_ptr() as _,
            function_name.len() as _,
            function_name.as_ptr() as _,
            arguments.len() as _,
            arguments.as_ptr() as _,
            &amount.as_yoctonear() as *const u128 as _,
            gas.as_gas(),
        ))
    }
}

/// Attaches the callback that is executed after promise pointed by `promise_idx` is complete.
pub fn promise_then(
    promise_idx: PromiseIndex,
    account_id: AccountId,
    function_name: &str,
    arguments: &[u8],
    amount: NearToken,
    gas: Gas,
) -> PromiseIndex {
    let account_id = account_id.as_bytes();
    unsafe {
        PromiseIndex(sys::promise_then(
            promise_idx.0,
            account_id.len() as _,
            account_id.as_ptr() as _,
            function_name.len() as _,
            function_name.as_ptr() as _,
            arguments.len() as _,
            arguments.as_ptr() as _,
            &amount.as_yoctonear() as *const u128 as _,
            gas.as_gas(),
        ))
    }
}

/// Creates a new promise which completes when time all promises passed as arguments complete.
pub fn promise_and(promise_indices: &[PromiseIndex]) -> PromiseIndex {
    let mut data = vec![0u8; size_of_val(promise_indices)];
    for i in 0..promise_indices.len() {
        data[i * size_of::<PromiseIndex>()..(i + 1) * size_of::<PromiseIndex>()]
            .copy_from_slice(&promise_indices[i].0.to_le_bytes());
    }
    unsafe { PromiseIndex(sys::promise_and(data.as_ptr() as _, promise_indices.len() as _)) }
}

pub fn promise_batch_create(account_id: &AccountId) -> PromiseIndex {
    let account_id: &str = account_id.as_ref();
    unsafe {
        PromiseIndex(sys::promise_batch_create(account_id.len() as _, account_id.as_ptr() as _))
    }
}

pub fn promise_batch_then(promise_index: PromiseIndex, account_id: &AccountId) -> PromiseIndex {
    let account_id: &str = account_id.as_ref();
    unsafe {
        PromiseIndex(sys::promise_batch_then(
            promise_index.0,
            account_id.len() as _,
            account_id.as_ptr() as _,
        ))
    }
}

pub fn promise_batch_action_create_account(promise_index: PromiseIndex) {
    unsafe { sys::promise_batch_action_create_account(promise_index.0) }
}

pub fn promise_batch_action_deploy_contract(promise_index: PromiseIndex, code: &[u8]) {
    unsafe {
        sys::promise_batch_action_deploy_contract(
            promise_index.0,
            code.len() as _,
            code.as_ptr() as _,
        )
    }
}

pub fn promise_batch_action_function_call(
    promise_index: PromiseIndex,
    function_name: &str,
    arguments: &[u8],
    amount: NearToken,
    gas: Gas,
) {
    unsafe {
        sys::promise_batch_action_function_call(
            promise_index.0,
            function_name.len() as _,
            function_name.as_ptr() as _,
            arguments.len() as _,
            arguments.as_ptr() as _,
            &amount.as_yoctonear() as *const u128 as _,
            gas.as_gas(),
        )
    }
}

pub fn promise_batch_action_function_call_weight(
    promise_index: PromiseIndex,
    function_name: &str,
    arguments: &[u8],
    amount: NearToken,
    gas: Gas,
    weight: GasWeight,
) {
    unsafe {
        sys::promise_batch_action_function_call_weight(
            promise_index.0,
            function_name.len() as _,
            function_name.as_ptr() as _,
            arguments.len() as _,
            arguments.as_ptr() as _,
            &amount.as_yoctonear() as *const u128 as _,
            gas.as_gas(),
            weight.0,
        )
    }
}

pub fn promise_batch_action_transfer(promise_index: PromiseIndex, amount: NearToken) {
    unsafe {
        sys::promise_batch_action_transfer(
            promise_index.0,
            &amount.as_yoctonear() as *const u128 as _,
        )
    }
}

pub fn promise_batch_action_stake(
    promise_index: PromiseIndex,
    amount: NearToken,
    public_key: &PublicKey,
) {
    unsafe {
        sys::promise_batch_action_stake(
            promise_index.0,
            &amount.as_yoctonear() as *const u128 as _,
            public_key.as_bytes().len() as _,
            public_key.as_bytes().as_ptr() as _,
        )
    }
}
pub fn promise_batch_action_add_key_with_full_access(
    promise_index: PromiseIndex,
    public_key: &PublicKey,
    nonce: u64,
) {
    unsafe {
        sys::promise_batch_action_add_key_with_full_access(
            promise_index.0,
            public_key.as_bytes().len() as _,
            public_key.as_bytes().as_ptr() as _,
            nonce,
        )
    }
}

/// This is a short lived function while we migrate between the Balance and the allowance type
pub(crate) fn migrate_to_allowance(allowance: NearToken) -> Allowance {
    Allowance::limited(allowance).unwrap_or(Allowance::Unlimited)
}

#[deprecated(since = "5.0.0", note = "Use add_access_key_allowance instead")]
pub fn promise_batch_action_add_key_with_function_call(
    promise_index: PromiseIndex,
    public_key: &PublicKey,
    nonce: u64,
    allowance: NearToken,
    receiver_id: &AccountId,
    function_names: &str,
) {
    let allowance = migrate_to_allowance(allowance);
    promise_batch_action_add_key_allowance_with_function_call(
        promise_index,
        public_key,
        nonce,
        allowance,
        receiver_id,
        function_names,
    )
}

pub fn promise_batch_action_add_key_allowance_with_function_call(
    promise_index: PromiseIndex,
    public_key: &PublicKey,
    nonce: u64,
    allowance: Allowance,
    receiver_id: &AccountId,
    function_names: &str,
) {
    let receiver_id: &str = receiver_id.as_ref();
    let allowance = match allowance {
        Allowance::Limited(x) => x.get(),
        Allowance::Unlimited => 0,
    };
    unsafe {
        sys::promise_batch_action_add_key_with_function_call(
            promise_index.0,
            public_key.as_bytes().len() as _,
            public_key.as_bytes().as_ptr() as _,
            nonce,
            &allowance as *const u128 as _,
            receiver_id.len() as _,
            receiver_id.as_ptr() as _,
            function_names.len() as _,
            function_names.as_ptr() as _,
        )
    }
}
pub fn promise_batch_action_delete_key(promise_index: PromiseIndex, public_key: &PublicKey) {
    unsafe {
        sys::promise_batch_action_delete_key(
            promise_index.0,
            public_key.as_bytes().len() as _,
            public_key.as_bytes().as_ptr() as _,
        )
    }
}

pub fn promise_batch_action_delete_account(
    promise_index: PromiseIndex,
    beneficiary_id: &AccountId,
) {
    let beneficiary_id: &str = beneficiary_id.as_ref();
    unsafe {
        sys::promise_batch_action_delete_account(
            promise_index.0,
            beneficiary_id.len() as _,
            beneficiary_id.as_ptr() as _,
        )
    }
}

/// If the current function is invoked by a callback we can access the execution results of the
/// promises that caused the callback. This function returns the number of complete and
/// incomplete callbacks.
pub fn promise_results_count() -> u64 {
    unsafe { sys::promise_results_count() }
}
/// If the current function is invoked by a callback we can access the execution results of the
/// promises that caused the callback.
pub fn promise_result(result_idx: u64) -> PromiseResult {
    match promise_result_internal(result_idx) {
        Ok(()) => {
            let data = expect_register(read_register(ATOMIC_OP_REGISTER));
            PromiseResult::Successful(data)
        }
        Err(PromiseError::Failed) => PromiseResult::Failed,
    }
}

pub(crate) fn promise_result_internal(result_idx: u64) -> Result<(), PromiseError> {
    match unsafe { sys::promise_result(result_idx, ATOMIC_OP_REGISTER) } {
        1 => Ok(()),
        2 => Err(PromiseError::Failed),
        _ => abort(),
    }
}
/// Consider the execution result of promise under `promise_idx` as execution result of this
/// function.
pub fn promise_return(promise_idx: PromiseIndex) {
    unsafe { sys::promise_return(promise_idx.0) }
}

// ###############
// # Validator API #
// ###############

/// For a given account return its current stake. If the account is not a validator, returns 0.
pub fn validator_stake(account_id: &AccountId) -> NearToken {
    let account_id: &str = account_id.as_ref();
    let data = [0u8; size_of::<NearToken>()];
    unsafe {
        sys::validator_stake(account_id.len() as _, account_id.as_ptr() as _, data.as_ptr() as u64)
    };
    NearToken::from_yoctonear(u128::from_le_bytes(data))
}

/// Returns the total stake of validators in the current epoch.
pub fn validator_total_stake() -> NearToken {
    let data = [0u8; size_of::<NearToken>()];
    unsafe { sys::validator_total_stake(data.as_ptr() as u64) };
    NearToken::from_yoctonear(u128::from_le_bytes(data))
}

// #####################
// # Miscellaneous API #
// #####################
/// Sets the blob of data as the return value of the contract.
pub fn value_return(value: &[u8]) {
    unsafe { sys::value_return(value.len() as _, value.as_ptr() as _) }
}
/// Terminates the execution of the program with the UTF-8 encoded message.
/// [`panic_str`] should be used as the bytes are required to be UTF-8
#[deprecated(since = "4.0.0", note = "Use env::panic_str to panic with a message.")]
pub fn panic(message: &[u8]) -> ! {
    unsafe { sys::panic_utf8(message.len() as _, message.as_ptr() as _) }
}

/// Terminates the execution of the program with the UTF-8 encoded message.
pub fn panic_str(message: &str) -> ! {
    unsafe { sys::panic_utf8(message.len() as _, message.as_ptr() as _) }
}

/// Aborts the current contract execution without a custom message.
/// To include a message, use [`panic_str`].
pub fn abort() -> ! {
    // Use wasm32 unreachable call to avoid including the `panic` external function in Wasm.
    #[cfg(target_arch = "wasm32")]
    //* This was stabilized recently (~ >1.51), so ignore warnings but don't enforce higher msrv
    #[allow(unused_unsafe)]
    unsafe {
        core::arch::wasm32::unreachable()
    }
    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        sys::panic()
    }
}

/// Logs the string message message. This message is stored on chain.
pub fn log_str(message: &str) {
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    eprintln!("{}", message);

    unsafe { sys::log_utf8(message.len() as _, message.as_ptr() as _) }
}

/// Log the UTF-8 encodable message.
#[deprecated(since = "4.0.0", note = "Use env::log_str for logging messages.")]
pub fn log(message: &[u8]) {
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    eprintln!("{}", String::from_utf8_lossy(message));

    unsafe { sys::log_utf8(message.len() as _, message.as_ptr() as _) }
}

// ###############
// # Storage API #
// ###############
/// Writes key-value into storage.
/// If another key-value existed in the storage with the same key it returns `true`, otherwise `false`.
///
/// # Examples
///
/// ```
/// use near_sdk::env::{storage_write, storage_read};
///
/// assert!(!storage_write(b"key", b"value"));
/// assert!(storage_write(b"key", b"another_value"));
/// assert_eq!(storage_read(b"key").unwrap(), b"another_value");
/// ```
pub fn storage_write(key: &[u8], value: &[u8]) -> bool {
    match unsafe {
        sys::storage_write(
            key.len() as _,
            key.as_ptr() as _,
            value.len() as _,
            value.as_ptr() as _,
            EVICTED_REGISTER,
        )
    } {
        0 => false,
        1 => true,
        _ => abort(),
    }
}
/// Reads the value stored under the given key.
///
/// # Examples
///
/// ```
/// use near_sdk::env::{storage_write, storage_read};
///
/// assert!(storage_read(b"key").is_none());
/// storage_write(b"key", b"value");
/// assert_eq!(storage_read(b"key").unwrap(), b"value");
/// ```
pub fn storage_read(key: &[u8]) -> Option<Vec<u8>> {
    match unsafe { sys::storage_read(key.len() as _, key.as_ptr() as _, ATOMIC_OP_REGISTER) } {
        0 => None,
        1 => Some(expect_register(read_register(ATOMIC_OP_REGISTER))),
        _ => abort(),
    }
}
/// Removes the value stored under the given key.
/// If key-value existed returns `true`, otherwise `false`.
pub fn storage_remove(key: &[u8]) -> bool {
    match unsafe { sys::storage_remove(key.len() as _, key.as_ptr() as _, EVICTED_REGISTER) } {
        0 => false,
        1 => true,
        _ => abort(),
    }
}
/// Reads the most recent value that was evicted with `storage_write` or `storage_remove` command.
pub fn storage_get_evicted() -> Option<Vec<u8>> {
    read_register(EVICTED_REGISTER)
}
/// Checks if there is a key-value in the storage.
pub fn storage_has_key(key: &[u8]) -> bool {
    match unsafe { sys::storage_has_key(key.len() as _, key.as_ptr() as _) } {
        0 => false,
        1 => true,
        _ => abort(),
    }
}

// ############################################
// # Saving and loading of the contract state #
// ############################################
/// Load the state of the given object.
pub fn state_read<T: borsh::BorshDeserialize>() -> Option<T> {
    storage_read(STATE_KEY)
        .map(|data| T::try_from_slice(&data).expect("Cannot deserialize the contract state."))
}

pub fn state_write<T: borsh::BorshSerialize>(state: &T) {
    let data = borsh::to_vec(state).expect("Cannot serialize the contract state.");
    storage_write(STATE_KEY, &data);
}

/// Returns `true` if the contract state exists and `false` otherwise.
pub fn state_exists() -> bool {
    storage_has_key(STATE_KEY)
}

// #####################################
// # Parameters exposed by the runtime #
// #####################################

/// Price per 1 byte of storage from mainnet genesis config.
/// TODO: will be using the host function when it will be available.

pub fn storage_byte_cost() -> NearToken {
    NearToken::from_yoctonear(10_000_000_000_000_000_000u128)
}

// ##################
// # Helper methods #
// ##################

/// Returns `true` if the given account ID is valid and `false` otherwise.
pub fn is_valid_account_id(account_id: &[u8]) -> bool {
    if (account_id.len() as u64) < MIN_ACCOUNT_ID_LEN
        || (account_id.len() as u64) > MAX_ACCOUNT_ID_LEN
    {
        return false;
    }

    // NOTE: We don't want to use Regex here, because it requires extra time to compile it.
    // The valid account ID regex is /^(([a-z\d]+[-_])*[a-z\d]+\.)*([a-z\d]+[-_])*[a-z\d]+$/
    // Instead the implementation is based on the previous character checks.

    // We can safely assume that last char was a separator.
    let mut last_char_is_separator = true;

    for c in account_id {
        let current_char_is_separator = match *c {
            b'a'..=b'z' | b'0'..=b'9' => false,
            b'-' | b'_' | b'.' => true,
            _ => return false,
        };
        if current_char_is_separator && last_char_is_separator {
            return false;
        }
        last_char_is_separator = current_char_is_separator;
    }
    // The account can't end as separator.
    !last_char_is_separator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_account_id_strings() {
        // Valid
        for account_id in &[
            "aa",
            "a-a",
            "a-aa",
            "100",
            "0o",
            "com",
            "near",
            "bowen",
            "b-o_w_e-n",
            "b.owen",
            "bro.wen",
            "a.ha",
            "a.b-a.ra",
            "system",
            "over.9000",
            "google.com",
            "illia.cheapaccounts.near",
            "0o0ooo00oo00o",
            "alex-skidanov",
            "10-4.8-2",
            "b-o_w_e-n",
            "no_lols",
            "0123456789012345678901234567890123456789012345678901234567890123",
            // Valid, but can't be created
            "near.a",
            "a.a",
        ] {
            assert!(
                is_valid_account_id(account_id.as_ref()),
                "Valid account id {:?} marked invalid",
                account_id
            );
        }

        // Invalid
        for account_id in &[
            "",
            "a",
            "A",
            "Abc",
            "-near",
            "near-",
            "-near-",
            "near.",
            ".near",
            "near@",
            "@near",
            "неар",
            "@@@@@",
            "0__0",
            "0_-_0",
            "0_-_0",
            "..",
            "a..near",
            "nEar",
            "_bowen",
            "hello world",
            "abcdefghijklmnopqrstuvwxyz.abcdefghijklmnopqrstuvwxyz.abcdefghijklmnopqrstuvwxyz",
            "01234567890123456789012345678901234567890123456789012345678901234",
            // `@` separators are banned now
            "some-complex-address@gmail.com",
            "sub.buy_d1gitz@atata@b0-rg.c_0_m",
        ] {
            assert!(
                !is_valid_account_id(account_id.as_ref()),
                "Invalid account id {:?} marked valid",
                account_id
            );
        }
    }

    #[test]
    fn test_is_valid_account_id_binary() {
        assert!(!is_valid_account_id(&[]));
        assert!(!is_valid_account_id(&[0]));
        assert!(!is_valid_account_id(&[0, 1]));
        assert!(!is_valid_account_id(&[0, 1, 2]));
        assert!(is_valid_account_id(b"near"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn hash_smoke_tests() {
        assert_eq!(
            &super::sha256_array(b"some value"),
            hex::decode("ab3d07f3169ccbd0ed6c4b45de21519f9f938c72d24124998aab949ce83bb51b")
                .unwrap()
                .as_slice()
        );

        assert_eq!(
            &super::keccak256_array(b"some value"),
            hex::decode("f928dfb5fc72b3bbfb9a5ccb0ee9843b27b4ac1ebc25a6f6f783e23ebd47ef1f")
                .unwrap()
                .as_slice()
        );

        assert_eq!(
            &super::keccak512_array(b"some value"),
            hex::decode("3e38d140a85123374ee63ec208973aa39b87349d17ccac948a2493e18b18b5913220cd174b4f511aa97977009e16be485fc94f5e2743cb9bb0579d35ab410583")
                .unwrap()
                .as_slice()
        );

        assert_eq!(
            &super::ripemd160_array(b"some value"),
            hex::decode("09f025fed704e1ecac8f88b2bda3e56876da03ac").unwrap().as_slice()
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn random_seed_smoke_test() {
        crate::testing_env!(crate::test_utils::VMContextBuilder::new()
            .random_seed([8; 32])
            .build());

        assert_eq!(super::random_seed(), [8; 32]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[cfg(feature = "unstable")]
    #[test]
    fn test_ecrecover() {
        use crate::test_utils::test_env;
        use hex::FromHex;
        use serde::de::Error;
        use serde::{Deserialize, Deserializer};
        use serde_json::from_slice;
        use std::fmt::Display;

        #[derive(Deserialize)]
        struct EcrecoverTest {
            #[serde(with = "hex::serde")]
            m: [u8; 32],
            v: u8,
            #[serde(with = "hex::serde")]
            sig: [u8; 64],
            mc: bool,
            #[serde(deserialize_with = "deserialize_option_hex")]
            res: Option<[u8; 64]>,
        }

        fn deserialize_option_hex<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
        where
            D: Deserializer<'de>,
            T: FromHex,
            <T as FromHex>::Error: Display,
        {
            Deserialize::deserialize(deserializer)
                .map(|v: Option<&str>| v.map(FromHex::from_hex).transpose().map_err(Error::custom))
                .and_then(|v| v)
        }

        test_env::setup_free();
        for EcrecoverTest { m, v, sig, mc, res } in
            from_slice::<'_, Vec<_>>(include_bytes!("../../tests/ecrecover-tests.json")).unwrap()
        {
            assert_eq!(super::ecrecover(&m, &sig, v, mc), res);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn signer_public_key() {
        let key: PublicKey =
            "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp".parse().unwrap();

        crate::testing_env!(crate::test_utils::VMContextBuilder::new()
            .signer_account_pk(key.clone())
            .build());
        assert_eq!(super::signer_account_pk(), key);
    }

    #[test]
    fn ed25519_verify() {
        const SIGNATURE: [u8; 64] = [
            145, 193, 203, 18, 114, 227, 14, 117, 33, 213, 121, 66, 130, 14, 25, 4, 36, 120, 46,
            142, 226, 215, 7, 66, 122, 112, 97, 30, 249, 135, 61, 165, 221, 249, 252, 23, 105, 40,
            56, 70, 31, 152, 236, 141, 154, 122, 207, 20, 75, 118, 79, 90, 168, 6, 221, 122, 213,
            29, 126, 196, 216, 104, 191, 6,
        ];

        const BAD_SIGNATURE: [u8; 64] = [1; 64];

        // create a forged signature with the `s` scalar not properly reduced
        // https://docs.rs/ed25519/latest/src/ed25519/lib.rs.html#302
        const FORGED_SIGNATURE: [u8; 64] = {
            let mut sig = SIGNATURE;
            sig[63] = 0b1110_0001;
            sig
        };

        const PUBLIC_KEY: [u8; 32] = [
            32, 122, 6, 120, 146, 130, 30, 37, 215, 112, 241, 251, 160, 196, 124, 17, 255, 75, 129,
            62, 84, 22, 46, 206, 158, 184, 57, 224, 118, 35, 26, 182,
        ];

        // create a forged public key to force a PointDecompressionError
        // https://docs.rs/ed25519-dalek/latest/src/ed25519_dalek/public.rs.html#142
        const FORGED_PUBLIC_KEY: [u8; 32] = {
            let mut key = PUBLIC_KEY;
            key[31] = 0b1110_0001;
            key
        };

        // 32 bytes message
        const MESSAGE: [u8; 32] = [
            107, 97, 106, 100, 108, 102, 107, 106, 97, 108, 107, 102, 106, 97, 107, 108, 102, 106,
            100, 107, 108, 97, 100, 106, 102, 107, 108, 106, 97, 100, 115, 107,
        ];

        assert!(super::ed25519_verify(&SIGNATURE, &MESSAGE, &PUBLIC_KEY));
        assert!(!super::ed25519_verify(&BAD_SIGNATURE, &MESSAGE, &FORGED_PUBLIC_KEY));
        assert!(!super::ed25519_verify(&SIGNATURE, &MESSAGE, &FORGED_PUBLIC_KEY));
        assert!(!super::ed25519_verify(&FORGED_SIGNATURE, &MESSAGE, &PUBLIC_KEY));
    }

    #[test]
    pub fn alt_bn128_g1_multiexp() {
        // Originated from https://github.com/near/nearcore/blob/8cd095ffc98a6507ed2d2a8982a6a3e42ebc1b62/runtime/near-test-contracts/estimator-contract/src/lib.rs#L557-L720
        let buffer = [
            16, 238, 91, 161, 241, 22, 172, 158, 138, 252, 202, 212, 136, 37, 110, 231, 118, 220,
            8, 45, 14, 153, 125, 217, 227, 87, 238, 238, 31, 138, 226, 8, 238, 185, 12, 155, 93,
            126, 144, 248, 200, 177, 46, 245, 40, 162, 169, 80, 150, 211, 157, 13, 10, 36, 44, 232,
            173, 32, 32, 115, 123, 2, 9, 47, 190, 148, 181, 91, 69, 6, 83, 40, 65, 222, 251, 70,
            81, 73, 60, 142, 130, 217, 176, 20, 69, 75, 40, 167, 41, 180, 244, 5, 142, 215, 135,
            35,
        ];

        assert_eq!(
            super::alt_bn128_g1_multiexp(&buffer),
            vec![
                150, 94, 159, 52, 239, 226, 181, 150, 77, 86, 90, 186, 102, 219, 243, 204, 36, 128,
                164, 209, 106, 6, 62, 124, 235, 104, 223, 195, 30, 204, 42, 20, 13, 158, 14, 197,
                133, 73, 43, 171, 28, 68, 82, 116, 244, 164, 36, 251, 244, 8, 234, 40, 118, 55,
                216, 187, 242, 39, 213, 160, 192, 184, 28, 23
            ]
        );
    }

    #[test]
    pub fn alt_bn128_g1_sum() {
        // Originated from https://github.com/near/nearcore/blob/8cd095ffc98a6507ed2d2a8982a6a3e42ebc1b62/runtime/near-test-contracts/estimator-contract/src/lib.rs#L557-L720
        let buffer = [
            0, 11, 49, 94, 29, 152, 111, 116, 138, 248, 2, 184, 8, 159, 80, 169, 45, 149, 48, 32,
            49, 37, 6, 133, 105, 171, 194, 120, 44, 195, 17, 180, 35, 137, 154, 4, 192, 211, 244,
            93, 200, 2, 44, 0, 64, 26, 108, 139, 147, 88, 235, 242, 23, 253, 52, 110, 236, 67, 99,
            176, 2, 186, 198, 228, 25,
        ];

        assert_eq!(
            super::alt_bn128_g1_sum(&buffer),
            vec![
                11, 49, 94, 29, 152, 111, 116, 138, 248, 2, 184, 8, 159, 80, 169, 45, 149, 48, 32,
                49, 37, 6, 133, 105, 171, 194, 120, 44, 195, 17, 180, 35, 137, 154, 4, 192, 211,
                244, 93, 200, 2, 44, 0, 64, 26, 108, 139, 147, 88, 235, 242, 23, 253, 52, 110, 236,
                67, 99, 176, 2, 186, 198, 228, 25
            ]
        );
    }

    #[test]
    pub fn alt_bn128_pairing_check() {
        // Taken from https://github.com/near/nearcore/blob/8cd095ffc98a6507ed2d2a8982a6a3e42ebc1b62/runtime/near-vm-runner/src/logic/tests/alt_bn128.rs#L239-L250
        let valid_pair = [
            117, 10, 217, 99, 113, 78, 234, 67, 183, 90, 26, 58, 200, 86, 195, 123, 42, 184, 213,
            88, 224, 248, 18, 200, 108, 6, 181, 6, 28, 17, 99, 7, 36, 134, 53, 115, 192, 180, 3,
            113, 76, 227, 174, 147, 50, 174, 79, 74, 151, 195, 172, 10, 211, 210, 26, 92, 117, 246,
            65, 237, 168, 104, 16, 4, 1, 26, 3, 219, 6, 13, 193, 115, 77, 230, 27, 13, 242, 214,
            195, 9, 213, 99, 135, 12, 160, 202, 114, 135, 175, 42, 116, 172, 79, 234, 26, 41, 212,
            111, 192, 129, 124, 112, 57, 107, 38, 244, 230, 222, 240, 36, 65, 238, 133, 188, 19,
            43, 148, 59, 205, 40, 161, 179, 173, 228, 88, 169, 231, 29, 17, 67, 163, 51, 165, 187,
            101, 44, 250, 24, 68, 101, 92, 128, 203, 190, 51, 85, 9, 43, 58, 136, 68, 180, 92, 110,
            185, 168, 107, 129, 45, 30, 187, 22, 100, 17, 75, 93, 216, 125, 23, 212, 11, 186, 199,
            204, 1, 140, 133, 11, 82, 44, 65, 222, 20, 26, 48, 26, 132, 220, 25, 213, 93, 25, 79,
            176, 4, 149, 151, 243, 11, 131, 253, 233, 121, 38, 222, 15, 118, 117, 200, 214, 175,
            233, 130, 181, 193, 167, 255, 153, 169, 240, 207, 235, 28, 31, 83, 74, 69, 179, 6, 150,
            72, 67, 74, 166, 130, 83, 82, 115, 123, 111, 208, 221, 64, 43, 237, 213, 186, 235, 7,
            56, 251, 179, 95, 233, 159, 23, 109, 173, 85, 103, 8, 165, 235, 226, 218, 79, 72, 120,
            172, 251, 20, 83, 121, 201, 140, 98, 170, 246, 121, 218, 19, 115, 42, 135, 60, 239, 30,
            32, 49, 170, 171, 204, 196, 197, 160, 158, 168, 47, 23, 110, 139, 123, 222, 222, 245,
            98, 125, 208, 70, 39, 110, 186, 146, 254, 66, 185, 118, 3, 78, 32, 47, 179, 197, 93,
            79, 240, 204, 78, 236, 133, 213, 173, 117, 94, 63, 154, 68, 89, 236, 138, 0, 247, 242,
            212, 245, 33, 249, 0, 35, 246, 233, 0, 124, 86, 198, 162, 201, 54, 19, 26, 196, 75,
            254, 71, 70, 238, 51, 2, 23, 185, 152, 139, 134, 65, 107, 129, 114, 244, 47, 251, 240,
            80, 193, 23,
        ];
        assert!(super::alt_bn128_pairing_check(&valid_pair));

        // Taken from https://github.com/near/nearcore/blob/8cd095ffc98a6507ed2d2a8982a6a3e42ebc1b62/runtime/near-vm-runner/src/logic/tests/alt_bn128.rs#L254-L265
        let invalid_pair = [
            117, 10, 217, 99, 113, 78, 234, 67, 183, 90, 26, 58, 200, 86, 195, 123, 42, 184, 213,
            88, 224, 248, 18, 200, 108, 6, 181, 6, 28, 17, 99, 7, 36, 134, 53, 115, 192, 180, 3,
            113, 76, 227, 174, 147, 50, 174, 79, 74, 151, 195, 172, 10, 211, 210, 26, 92, 117, 246,
            65, 237, 168, 104, 16, 4, 1, 26, 3, 219, 6, 13, 193, 115, 77, 230, 27, 13, 242, 214,
            195, 9, 213, 99, 135, 12, 160, 202, 114, 135, 175, 42, 116, 172, 79, 234, 26, 41, 212,
            111, 192, 129, 124, 112, 57, 107, 38, 244, 230, 222, 240, 36, 65, 238, 133, 188, 19,
            43, 148, 59, 205, 40, 161, 179, 173, 228, 88, 169, 231, 29, 17, 67, 163, 51, 165, 187,
            101, 44, 250, 24, 68, 101, 92, 128, 203, 190, 51, 85, 9, 43, 58, 136, 68, 180, 92, 110,
            185, 168, 107, 129, 45, 30, 187, 22, 100, 17, 75, 93, 216, 125, 23, 212, 11, 186, 199,
            204, 1, 140, 133, 11, 82, 44, 65, 222, 20, 26, 48, 26, 132, 220, 25, 213, 93, 25, 117,
            10, 217, 99, 113, 78, 234, 67, 183, 90, 26, 58, 200, 86, 195, 123, 42, 184, 213, 88,
            224, 248, 18, 200, 108, 6, 181, 6, 28, 17, 99, 7, 36, 134, 53, 115, 192, 180, 3, 113,
            76, 227, 174, 147, 50, 174, 79, 74, 151, 195, 172, 10, 211, 210, 26, 92, 117, 246, 65,
            237, 168, 104, 16, 4, 109, 173, 85, 103, 8, 165, 235, 226, 218, 79, 72, 120, 172, 251,
            20, 83, 121, 201, 140, 98, 170, 246, 121, 218, 19, 115, 42, 135, 60, 239, 30, 32, 49,
            170, 171, 204, 196, 197, 160, 158, 168, 47, 23, 110, 139, 123, 222, 222, 245, 98, 125,
            208, 70, 39, 110, 186, 146, 254, 66, 185, 118, 3, 78, 32, 47, 179, 197, 93, 79, 240,
            204, 78, 236, 133, 213, 173, 117, 94, 63, 154, 68, 89, 236, 138, 0, 247, 242, 212, 245,
            33, 249, 0, 35, 246, 233, 0, 124, 86, 198, 162, 201, 54, 19, 26, 196, 75, 254, 71, 70,
            238, 51, 2, 23, 185, 152, 139, 134, 65, 107, 129, 114, 244, 47, 251, 240, 80, 193, 23,
        ];

        assert!(!super::alt_bn128_pairing_check(&invalid_pair));
    }
}
