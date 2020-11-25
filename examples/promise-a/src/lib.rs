use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, ext_contract, log, near_bindgen, AccountId, Balance, Gas, Promise};

#[global_allocator]
static ALLOC: near_sdk::wee_alloc::WeeAlloc<'_> = near_sdk::wee_alloc::WeeAlloc::INIT;

#[near_bindgen]
#[derive(Default, BorshDeserialize, BorshSerialize)]
pub struct PromiseA {}

const NO_DEPOSIT: Balance = 0;

const BASIC_GAS: Gas = 5_000_000_000_000;

const ALICE: &str = "a.place.meta";
const BOB: &str = "b.place.meta";

#[ext_contract(ext_bob)]
pub trait Bob {
    fn return_123(&self) -> String;
}

#[ext_contract(ext_self_alice)]
pub trait SelfAlice {
    fn alice_on_data(&mut self, #[callback] data: String) -> String;
}

fn log_it(s: &str) {
    log!("I'm @{}. Called by @{}. {}", env::current_account_id(), env::predecessor_account_id(), s);
}

#[near_bindgen]
impl PromiseA {
    pub fn example_1(&mut self) -> Promise {
        log_it("example_1: alice calls bob with callback");

        ext_bob::return_123(&BOB, NO_DEPOSIT, BASIC_GAS).then(ext_self_alice::alice_on_data(
            &env::current_account_id(),
            NO_DEPOSIT,
            BASIC_GAS,
        ))
    }

    pub fn alice_on_data(&mut self, #[callback] data: String) -> String {
        log_it(format!("alice_on_data with data '{}'", data).as_str());
        format!("on_data '{}'", data)
    }
}
