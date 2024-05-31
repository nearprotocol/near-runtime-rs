use near_sdk_macros::contract_error;

#[contract_error(inside_nearsdk)]
pub struct InvalidArgument {
    pub message: String,
}

impl InvalidArgument {
    pub fn new(message: &str) -> Self {
        Self { message: message.to_string() }
    }
}

#[contract_error(inside_nearsdk)]
pub struct InvalidContractState {
    pub message: String,
}

impl InvalidContractState {
    pub fn new(message: &str) -> Self {
        Self { message: message.to_string() }
    }
}

#[contract_error(inside_nearsdk)]
pub struct PermissionDenied {
    message: Option<String>,
}

impl PermissionDenied {
    pub fn new(message: Option<&str>) -> Self {
        Self { message: message.map(|s| s.to_string()) }
    }
}

#[contract_error(inside_nearsdk)]
pub struct ContractUpgradeError {
    pub message: String,
}

impl ContractUpgradeError {
    pub fn new(message: &str) -> Self {
        Self { message: message.to_string() }
    }
}
#[contract_error(inside_nearsdk)]
#[derive(Default)]
pub struct RequireFailed {
    pub message: String,
}

impl RequireFailed {
    pub fn new() -> Self {
        Self { message: "require! assertion failed".to_string() }
    }
}

#[contract_error(inside_nearsdk)]
pub struct PromiseFailed {
    pub promise_index: Option<u64>,
    pub message: Option<String>,
}

impl PromiseFailed {
    pub fn new(promise_index: Option<u64>, message: Option<&str>) -> Self {
        Self { promise_index, message: message.map(|s| s.to_string()) }
    }
}

#[contract_error(inside_nearsdk)]
pub struct InvalidPromiseReturn {
    pub message: String,
}

impl InvalidPromiseReturn {
    pub fn new(message: &str) -> Self {
        Self { message: message.to_string() }
    }
}

#[contract_error(inside_nearsdk)]
pub struct InsufficientBalance {
    message: Option<String>,
}

impl InsufficientBalance {
    pub fn new(message: Option<&str>) -> Self {
        Self { message: message.map(|s| s.to_string()) }
    }
}

#[contract_error(inside_nearsdk)]
pub struct InsufficientGas {}

#[contract_error(inside_nearsdk)]
pub struct TotalSupplyOverflow {}

#[contract_error(inside_nearsdk)]
pub struct UnexpectedFailure {
    pub message: String,
}

#[contract_error(inside_nearsdk)]
pub struct ContractError {
    pub message: String,
}

impl ContractError {
    pub fn new(message: &str) -> Self {
        Self { message: message.to_string() }
    }
}

#[contract_error(inside_nearsdk)]
pub struct InvalidHashLength {
    pub expected: usize,
}

impl InvalidHashLength {
    pub fn new(expected: usize) -> Self {
        Self { expected }
    }
}
