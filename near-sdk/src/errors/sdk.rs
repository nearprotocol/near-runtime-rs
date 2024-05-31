use near_sdk_macros::contract_error;

#[contract_error(inside_nearsdk, sdk)]
pub struct DepositNotAccepted {
    pub method: String,
}

impl DepositNotAccepted {
    pub fn new(method: &str) -> Self {
        Self { method: method.to_string() }
    }
}

#[contract_error(inside_nearsdk, sdk)]
pub struct ContractNotInitialized {}

#[contract_error(inside_nearsdk, sdk)]
pub struct ContractAlreadyInitialized {}

#[contract_error(inside_nearsdk, sdk)]
pub struct CallbackComputationUnsuccessful {
    pub index: u64,
}

impl CallbackComputationUnsuccessful {
    pub fn new(index: u64) -> Self {
        Self { index }
    }
}

#[contract_error(inside_nearsdk, sdk)]
pub struct ActionInJointPromise {
    pub message: String,
}

impl ActionInJointPromise {
    pub fn new() -> Self {
        Self { message: "Cannot add action to a joint promise.".to_string() }
    }
}

impl Default for ActionInJointPromise {
    fn default() -> Self {
        Self::new()
    }
}

#[contract_error(inside_nearsdk, sdk)]
pub struct PromiseAlreadyScheduled {
    pub message: String,
}

impl PromiseAlreadyScheduled {
    pub fn new() -> Self {
        Self {
            message: "Cannot callback promise which is already scheduled after another".to_string(),
        }
    }
}

impl Default for PromiseAlreadyScheduled {
    fn default() -> Self {
        Self::new()
    }
}

#[contract_error(inside_nearsdk, sdk)]
pub struct CallbackJointPromise {
    pub message: String,
}

impl CallbackJointPromise {
    pub fn new() -> Self {
        Self { message: "Cannot callback joint promise".to_string() }
    }
}

impl Default for CallbackJointPromise {
    fn default() -> Self {
        Self::new()
    }
}

#[contract_error(inside_nearsdk, sdk)]
pub struct PrivateMethod {
    pub method_name: String,
}

impl PrivateMethod {
    pub fn new(method_name: &str) -> Self {
        Self { method_name: method_name.to_string() }
    }
}

#[contract_error(inside_nearsdk, sdk)]
pub struct BorshSerializeError {
    subject: String,
}

impl BorshSerializeError {
    pub fn new(subject: &str) -> Self {
        Self { subject: subject.to_string() }
    }
}

#[contract_error(inside_nearsdk, sdk)]
pub struct BorshDeserializeError {
    subject: String,
}

impl BorshDeserializeError {
    pub fn new(subject: &str) -> Self {
        Self { subject: subject.to_string() }
    }
}

#[contract_error(inside_nearsdk, sdk)]
pub struct InvalidTreeMapRange {}

#[contract_error(inside_nearsdk, sdk)]
pub struct InconsistentCollectionState {
    pub message: String,
}

impl InconsistentCollectionState {
    pub fn new() -> Self {
        Self {
            message: "The collection is an inconsistent state. Did previous smart contract execution terminate unexpectedly?".to_string()
        }
    }
}

impl Default for InconsistentCollectionState {
    fn default() -> Self {
        Self::new()
    }
}

#[contract_error(inside_nearsdk, sdk)]
pub struct IndexOutOfBounds {}

#[contract_error(inside_nearsdk, sdk)]
pub struct KeyNotFound {}

#[contract_error(inside_nearsdk, sdk)]
pub struct RegisterEmpty {
    pub message: String,
}

impl RegisterEmpty {
    pub fn new() -> Self {
        Self {
            message: "Register was expected to have data because we just wrote it into it."
                .to_string(),
        }
    }
}

impl Default for RegisterEmpty {
    fn default() -> Self {
        Self::new()
    }
}
