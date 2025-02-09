pub mod macro_type_inspection;

#[cfg(feature = "abi")]
pub use near_abi::__private::ChunkedAbiEntry;
#[cfg(feature = "abi")]
pub use near_abi::{
    AbiBorshParameter, AbiFunction, AbiFunctionKind, AbiFunctionModifier, AbiJsonParameter,
    AbiParameters, AbiType,
};
#[cfg(feature = "abi")]
mod result_type_ext;

#[cfg(feature = "abi")]
pub use result_type_ext::ResultTypeExt;

use crate::IntoStorageKey;
use borsh::{to_vec, BorshSerialize};

/// Converts a Borsh serializable object into a `Vec<u8>` that is used for a storage key.
///
/// [`BorshStorageKey`](crate::BorshStorageKey) should be used instead of implementing
/// this manually.
///
/// ```
/// use near_sdk::borsh::BorshSerialize;
/// use near_sdk::BorshStorageKey;
/// use near_sdk::collections::LookupMap;
///
/// #[derive(BorshSerialize, BorshStorageKey)]
///  enum StorageKey {
///     FungibleToken,
///     Metadata { sub_key: String },
/// }
///
/// let lookup_map_1: LookupMap<u64, String> = LookupMap::new(StorageKey::Metadata { sub_key: String::from("yo") });
/// let lookup_map_2: LookupMap<String, String> = LookupMap::new(StorageKey::FungibleToken);
/// ```
pub trait BorshIntoStorageKey: BorshSerialize {}

impl<T> IntoStorageKey for T
where
    T: BorshIntoStorageKey,
{
    fn into_storage_key(self) -> Vec<u8> {
        to_vec(&self).unwrap()
    }
}
