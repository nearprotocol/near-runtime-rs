/// Fungible tokens as described in [by the spec](https://nomicon.io/Standards/FungibleToken/README.html).
pub mod fungible_token;
/// Non-fungible tokens as described in [by the spec](https://nomicon.io/Standards/NonFungibleToken/README.html).
pub mod non_fungible_token;
/// Storage management deals with handling [state storage](https://docs.near.org/docs/concepts/storage-staking) on NEAR. This follows the [storage management standard](https://nomicon.io/Standards/StorageManagement.html).
pub mod storage_management;
/// This upgrade standard is a use case where a staging area exists for a WASM
/// blob, allowing it to be stored for a period of time before deployed.
#[deprecated(
    since = "4.1.0",
    note = "This was removed because there is no standard (NEP) for upgradable contracts."
)]
pub mod upgrade;

pub(crate) mod event;

pub(crate) const ERR_ARITHMETIC_OVERFLOW: &str = "arithmetic overflow in contract standards";
