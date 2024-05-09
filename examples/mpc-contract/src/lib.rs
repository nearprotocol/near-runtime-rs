use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::TreeMap;
use near_sdk::{
    env, log, near_bindgen, serde_json, AccountId, BorshStorageKey, CryptoHash, Gas, GasWeight,
    NearToken, PromiseError,
};

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
struct RecordsKey;

#[derive(BorshDeserialize, BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
pub struct SignatureRequest {
    data_id: CryptoHash,
    account_id: AccountId,
    message: String,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
pub struct MpcContract {
    requests: TreeMap<u64, SignatureRequest>,
    next_available_request_index: u64,
}

impl Default for MpcContract {
    fn default() -> Self {
        Self { requests: TreeMap::new(RecordsKey), next_available_request_index: 0u64 }
    }
}

// Register used to receive data id from `promise_await_data`.
const DATA_ID_REGISTER: u64 = 0;

// Prepaid gas for a `sign_on_finish` call
const SIGN_ON_FINISH_CALL_GAS: Gas = Gas::from_tgas(5);

// Prepaid gas for a `do_something` call
const CHAINED_CALL_GAS: Gas = Gas::from_tgas(5);

#[near_bindgen]
impl MpcContract {
    /// User-facing API: accepts some message and returns a signature
    pub fn sign(&mut self, message_to_be_signed: String) {
        let index = self.next_available_request_index;
        self.next_available_request_index += 1;

        let yield_promise = env::promise_yield_create(
            "sign_on_finish",
            &serde_json::to_vec(&(index,)).unwrap(),
            SIGN_ON_FINISH_CALL_GAS,
            GasWeight(0),
            DATA_ID_REGISTER,
        );

        // Store the request in the contract's local state
        let data_id: CryptoHash =
            env::read_register(DATA_ID_REGISTER).expect("").try_into().expect("");
        self.requests.insert(
            &index,
            &SignatureRequest {
                data_id,
                account_id: env::signer_account_id(),
                message: message_to_be_signed,
            },
        );

        // The yield promise is composable with the usual promise API features. We can choose to
        // chain another function call and it will receive the output of the `sign_on_finish`
        // callback. Note that this chained promise can be a cross-contract call.
        env::promise_then(
            yield_promise,
            env::current_account_id(),
            "do_something",
            &[],
            NearToken::from_near(0),
            CHAINED_CALL_GAS,
        );

        // The return value for this function call will be the value
        // returned by the `sign_on_finish` callback.
        env::promise_return(yield_promise);
    }

    /// Called by MPC participants to submit a signature
    pub fn sign_respond(&mut self, data_id: String, signature: String) {
        let mut data_id_buf = [0u8; 32];
        hex::decode_to_slice(data_id, &mut data_id_buf).expect("");
        let data_id = data_id_buf;

        // check that caller is allowed to respond, signature is valid, etc.
        // ...

        log!("submitting response {} for data id {:?}", &signature, &data_id);
        env::promise_yield_resume(&data_id, &serde_json::to_vec(&signature).unwrap());
    }

    /// Callback receiving the externally submitted data (or a PromiseError)
    pub fn sign_on_finish(
        &mut self,
        request_index: u64,
        #[callback_result] signature: Result<String, PromiseError>,
    ) -> String {
        // Clean up the local state
        self.requests.remove(&request_index);

        match signature {
            Ok(signature) => "signature received: ".to_owned() + &signature,
            Err(_) => "signature request timed out".to_string(),
        }
    }

    pub fn do_something(#[callback_unwrap] signature_result: String) {
        log!("fn do_something invoked with result '{}'", &signature_result);
    }

    /// Helper for local testing; prints all pending requests
    pub fn log_pending_requests(&self) {
        for (_, request) in self.requests.iter() {
            log!(
                "{}: account_id={} payload={}",
                hex::encode(request.data_id),
                request.account_id,
                request.message
            );
        }
    }
}
