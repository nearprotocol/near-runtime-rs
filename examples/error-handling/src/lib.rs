// Find all our documentation at https://docs.near.org
use near_sdk::contract_error;
use near_sdk::near;
use near_sdk::BaseError;

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
    #[init]
    pub fn new() -> Self {
        Self { value: 0 }
    }

    // Examples of RPC response for function call:
    // is_error = false
    // --- Result -------------------------
    // 1
    // ------------------------------------
    // (changes value from 0 to 1)
    //
    // is_error = true
    // Failed transaction
    // Error:
    // 0: Error: An error occurred during a `FunctionCall` Action, parameter is debug message.
    // ExecutionError("Smart contract panicked: error in inc_handle_result")
    // (does not change value)
    #[handle_result]
    pub fn inc_handle_result(&mut self, is_error: bool) -> Result<u32, &'static str> {
        self.value += 1;
        if is_error {
            Err("error in inc_handle_result")
        } else {
            Ok(self.value)
        }
    }

    // Examples of RPC response for function call:
    // is_error = false
    // --- Result -------------------------
    // 2
    // ------------------------------------
    // (changes value from 1 to 2)
    //
    // is_error = true
    // Failed transaction
    // Error:
    // 0: Error: An error occurred during a `FunctionCall` Action, parameter is debug message.
    // ExecutionError("Smart contract panicked: {\"error\":{\"error_type\":\"error_handling::MyErrorEnum\",\"value\":\"X\"}}")
    // (changes value from 2 to 3)
    #[persist_on_error]
    pub fn inc_persist_on_err(&mut self, is_error: bool) -> Result<u32, MyErrorEnum> {
        self.value += 1;
        if is_error {
            Err(MyErrorEnum::X)
        } else {
            Ok(self.value)
        }
    }

    // Examples of RPC response for function call:
    // is_error = false
    // --- Result -------------------------
    // 4
    // ------------------------------------
    // (changes value from 3 to 4)
    //
    // is_error = true
    // Failed transaction
    // Error:
    // 0: Error: An error occurred during a `FunctionCall` Action, parameter is debug message.
    //  ExecutionError("Smart contract panicked: {\"error\":{\"error_type\":\"error_handling::MyErrorStruct\",\"value\":{\"x\":5}}}")
    // (does not change value)
    pub fn inc_just_result(&mut self, is_error: bool) -> Result<u32, MyErrorStruct> {
        self.value += 1;
        if is_error {
            Err(MyErrorStruct { x: 5 })
        } else {
            Ok(self.value)
        }
    }

    // Examples of RPC response for function call:
    // is_error = false
    // --- Result -------------------------
    // 5
    // ------------------------------------
    // (changes value from 4 to 5)
    //
    // is_error = true
    // Failed transaction
    // Error:
    // 0: Error: An error occurred during a `FunctionCall` Action, parameter is debug message.
    //  ExecutionError("Smart contract panicked: Error")
    // (does not change value)
    pub fn inc_just_simple(&mut self, is_error: bool) -> u32 {
        self.value += 1;
        if is_error {
            ::near_sdk::env::panic_str("Error");
        } else {
            self.value
        }
    }

    // Examples of RPC response for function call:
    // is_error = false
    // --- Result -------------------------
    // 6
    // ------------------------------------
    // (changes value from 5 to 6)
    //
    // is_error = true
    // Failed transaction
    // Error:
    // 0: Error: An error occurred during a `FunctionCall` Action, parameter is debug message.
    //  ExecutionError("Smart contract panicked: {\\\"error\\\":{\\\"cause\\\":{\\\"info\\\":{\\\"error\\\":{\\\"x\\\":5}},\\\"name\\\":\\\"near_sdk::utils::contract_error::BaseError\\\"},\\\"name\\\":\\\"CUSTOM_CONTRACT_ERROR\\\"}}")
    // (does not change value)
    pub fn inc_base_error(&mut self, is_error: bool) -> Result<u32, BaseError> {
        self.value += 1;
        if is_error {
            Err(MyErrorStruct { x: 5 }.into())
        } else {
            Ok(self.value)
        }
    }

    // Does not compile as u64 is not marked with contract_error
    // > the trait `ContractErrorTrait` is not implemented for `u64`
    // pub fn inc_incorrect_result_type(&mut self, is_error: bool) -> Result<u32, u64> {
    //     self.value += 1;
    //     if is_error {
    //         Err(0)
    //     } else {
    //         Ok(self.value)
    //     }
    // }

    pub fn get_value(&self) -> u32 {
        self.value
    }
}
