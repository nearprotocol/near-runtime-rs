// Find all our documentation at https://docs.near.org
use near_sdk::contract_error;
use near_sdk::near;

#[contract_error]
pub enum MyErrorEnum {
    X,
}

#[contract_error(sdk)]
pub struct MyErrorStruct {
    x: u32,
}

#[near(contract_state)]
#[derive(Default)]
pub struct Contract {
    value: u32,
}

#[near]
impl Contract {
    pub fn inc_incorrect_result_type(&mut self) -> Result<u32, u64> {
        Err(0)
    }
}

fn main() {}
