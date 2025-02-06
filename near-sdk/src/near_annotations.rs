//! `#[near]` and `#[near_bindgen]` documentation module
//!
//! This is not a real module; here we document the attributes that [`#[near]`](crate::near)
//! and [`#[near_bindgen]`](crate::near_bindgen) macro use.
//!
//! `near_bindgen` and `near_sdk` shares almost the same attributes:
//! * init
//! * payable
//! * private
//! * handle_result
//! * event_json
//! * contract_metadata
//! * serializer
//!
//! These attributes are only part of the `near` macro.
//! * serializers
//! * contract_state

/// See [`near_sdk::near #[init]`](crate::near#init-annotates-methods-of-a-type-in-its-impl-block) for details.
pub fn init() {}

/// See [`near_sdk::near #[payable]`](crate::near#payable-annotates-methods-of-a-type-in-its-impl-block)
pub fn payable() {}

/// See [`near_sdk::near #[private]`](crate::near#private-annotates-methods-of-a-type-in-its-impl-block)
pub fn private() {}

/// See [`near_sdk::near #[result_serializer]`](crate::near#result_serializer-annotates-methods-of-a-type-in-its-impl-block)
pub fn result_serializer() {}

/// See [`near_sdk::near #[handle_result]`](crate::near#handle_result-annotates-methods-of-a-type-in-its-impl-block)
pub fn handle_result() {}

/// See [`near_sdk::near #[near(event_json(...))]`](crate::near#nearevent_json-annotates-enums)
pub fn event_json() {}

/// See [`near_sdk::near #[near(contract_metadata(...))]`](crate::near#nearcontract_metadata-annotates-structsenums)
pub fn contract_metadata() {}

/// See [`near_sdk::near #[serializer(...)]`](crate::near#serializer-annotates-function-arguments)
pub fn serializer() {}

/// See [`near_sdk::near #[near(serializers=[...])]`](crate::near#nearserializers-annotates-structsenums)
pub fn serializers() {}

/// See [`near_sdk::near #[near(contract_state)]`](crate::near#nearcontract_state-annotates-structsenums)
pub fn contract_state() {}
