//! Complex smart contract.

use borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::near_bindgen;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(
    BorshDeserialize, BorshSerialize, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum TypeA {
    Var1,
    Var2,
}

#[derive(
    BorshDeserialize, BorshSerialize, Eq, PartialEq, Hash, PartialOrd, Serialize, Deserialize,
)]
pub enum TypeB {
    Var1,
    Var2,
}

#[near(contract_state)]
#[derive(Default, BorshDeserialize, BorshSerialize)]
struct Storage {
    map: HashMap<TypeA, TypeB>,
}

#[near]
impl Storage {
    pub fn insert(&mut self, key: TypeA, value: TypeB) -> Option<TypeB> {
        self.map.insert(key, value)
    }
}

fn main() {}
