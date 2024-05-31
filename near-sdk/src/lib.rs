//* Clippy is giving false positive warnings for this in 1.57 version. Remove this if fixed.
//* https://github.com/rust-lang/rust-clippy/issues/8091
#![allow(clippy::redundant_closure)]
// We want to enable all clippy lints, but some of them generate false positives.
#![allow(clippy::missing_const_for_fn, clippy::redundant_pub_crate)]
#![allow(clippy::multiple_bound_locations)]

#[cfg(test)]
extern crate quickcheck;

pub use near_sdk_macros::{
    contract_error, ext_contract, near, near_bindgen, BorshStorageKey, EventMetadata,
    FunctionError, NearSchema, PanicOnDefault,
};

pub mod store;

#[cfg(feature = "legacy")]
pub mod collections;
mod environment;
pub use environment::env;

#[cfg(feature = "unstable")]
pub use near_sys as sys;

mod promise;
pub use promise::{Allowance, Promise, PromiseOrValue};

// Private types just used within macro generation, not stable to be used.
#[doc(hidden)]
#[path = "private/mod.rs"]
pub mod __private;

pub mod json_types;

mod types;
pub use crate::types::*;

pub mod errors;

#[cfg(all(feature = "unit-testing", not(target_arch = "wasm32")))]
pub use environment::mock;
#[cfg(all(feature = "unit-testing", not(target_arch = "wasm32")))]
pub use environment::mock::test_vm_config;
#[cfg(all(feature = "unit-testing", not(target_arch = "wasm32")))]
// Re-export to avoid breakages
pub use environment::mock::MockedBlockchain;
#[cfg(all(feature = "unit-testing", not(target_arch = "wasm32")))]
pub use test_utils::context::VMContext;

pub mod utils;
pub use crate::utils::storage_key_impl::IntoStorageKey;
pub use crate::utils::*;

#[cfg(all(feature = "unit-testing", not(target_arch = "wasm32")))]
pub mod test_utils;

// Set up global allocator by default if custom-allocator feature is not set in wasm32 architecture.
#[cfg(all(feature = "wee_alloc", target_arch = "wasm32"))]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// Exporting common crates

pub use base64;
pub use borsh;
pub use bs58;
#[cfg(feature = "abi")]
pub use schemars;
pub use serde;
pub use serde_json;
