pub trait ContractErrorTrait {
    fn error_type(&self) -> &'static str;
    fn wrap(&self) -> serde_json::Value;
}

pub fn check_contract_error_trait<T: ContractErrorTrait>(_: &T) {}

#[crate::contract_error(inside_nearsdk)]
pub struct BaseError {
    #[serde(flatten)]
    pub error: serde_json::Value,
}

impl From<BaseError> for String {
    fn from(value: BaseError) -> Self {
        value.error.to_string()
    }
}

pub fn wrap_error<T: ContractErrorTrait>(error: T) -> serde_json::Value {
    error.wrap()
}
