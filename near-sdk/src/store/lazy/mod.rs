//! A persistent lazy storage value. Stores a value for a given key.
//! Example:
//! If the underlying value is large, e.g. the contract needs to store an image, but it doesn't need
//! to have access to this image at regular calls, then the contract can wrap this image into
//! [`Lazy`] and it will not be deserialized until requested.

mod impls;

use borsh::{to_vec, BorshDeserialize, BorshSerialize};
use once_cell::unsync::OnceCell;

use near_sdk_macros::near;

use crate::utils::{CacheEntry, EntryState};
use crate::{env, errors, IntoStorageKey};

fn expect_key_exists<T>(val: Option<T>) -> T {
    val.unwrap_or_else(|| env::panic_err(errors::KeyNotFound {}.into()))
}

fn expect_consistent_state<T>(val: Option<T>) -> T {
    val.unwrap_or_else(|| env::panic_err(errors::InconsistentCollectionState::new().into()))
}

pub(crate) fn load_and_deserialize<T>(key: &[u8]) -> CacheEntry<T>
where
    T: BorshDeserialize,
{
    let bytes = expect_key_exists(env::storage_read(key));
    let val = T::try_from_slice(&bytes)
        .unwrap_or_else(|_| env::panic_err(errors::BorshDeserializeError::new("value").into()));
    CacheEntry::new_cached(Some(val))
}

pub(crate) fn serialize_and_store<T>(key: &[u8], value: &T)
where
    T: BorshSerialize,
{
    let serialized = to_vec(value)
        .unwrap_or_else(|_| env::panic_err(errors::BorshSerializeError::new("value").into()));
    env::storage_write(key, &serialized);
}

/// An persistent lazily loaded value, that stores a value in the storage.
///
/// This will only write to the underlying store if the value has changed, and will only read the
/// existing value from storage once.
///
/// # Examples
/// ```
/// use near_sdk::store::Lazy;
///
/// let mut a = Lazy::new(b"a", "test string".to_string());
/// assert_eq!(*a, "test string");
///
/// *a = "new string".to_string();
/// assert_eq!(a.get(), "new string");
/// ```
#[near(inside_nearsdk)]
pub struct Lazy<T>
where
    T: BorshSerialize,
{
    /// Key bytes to index the contract's storage.
    storage_key: Box<[u8]>,
    #[borsh(skip, bound(deserialize = ""))] // removes `core::default::Default` bound from T
    /// Cached value which is lazily loaded and deserialized from storage.
    cache: OnceCell<CacheEntry<T>>,
}

impl<T> Lazy<T>
where
    T: BorshSerialize,
{
    /// Initializes new lazily loaded value with a given storage prefix and the value to initialize
    /// it with.
    ///
    /// This prefix can be anything that implements [`IntoStorageKey`]. The prefix is used when
    /// storing and looking up values in storage to ensure no collisions with other collections.
    pub fn new<S>(key: S, value: T) -> Self
    where
        S: IntoStorageKey,
    {
        Self {
            storage_key: key.into_storage_key().into_boxed_slice(),
            cache: OnceCell::from(CacheEntry::new_modified(Some(value))),
        }
    }

    /// Updates the value with a new value. This does not load the current value from storage.
    pub fn set(&mut self, value: T) {
        if let Some(v) = self.cache.get_mut() {
            *v.value_mut() = Some(value);
        } else {
            self.cache.set(CacheEntry::new_modified(Some(value))).unwrap_or_else(|_| {
                env::panic_err(
                    errors::ContractError::new("cache is checked to not be filled above").into(),
                )
            })
        }
    }

    /// Writes any changes to the value to storage. This will automatically be done when the
    /// value is dropped through [`Drop`] so this should only be used when the changes need to be
    /// reflected in the underlying storage before then.
    pub fn flush(&mut self) {
        if let Some(v) = self.cache.get_mut() {
            if v.is_modified() {
                // Value was modified, serialize and put the serialized bytes in storage.
                let value = expect_consistent_state(v.value().as_ref());
                serialize_and_store(&self.storage_key, value);

                // Replaces cache entry state to cached because the value in memory matches the
                // stored value. This avoids writing the same value twice.
                v.replace_state(EntryState::Cached);
            }
        }
    }
}

impl<T> Lazy<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    /// Returns a reference to the lazily loaded storage value.
    /// The load from storage only happens once, and if the value is already cached, it will not
    /// be reloaded.
    ///
    /// This function will panic if the cache is not loaded and the value at the key does not exist.
    pub fn get(&self) -> &T {
        let entry = self.cache.get_or_init(|| load_and_deserialize(&self.storage_key));

        expect_consistent_state(entry.value().as_ref())
    }

    /// Returns a reference to the lazily loaded storage value.
    /// The load from storage only happens once, and if the value is already cached, it will not
    /// be reloaded.
    ///
    /// This function will panic if the cache is not loaded and the value at the key does not exist.
    pub fn get_mut(&mut self) -> &mut T {
        self.cache.get_or_init(|| load_and_deserialize(&self.storage_key));
        let entry = self.cache.get_mut().unwrap_or_else(|| env::abort());

        expect_consistent_state(entry.value_mut().as_mut())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_lazy() {
        let mut a = Lazy::new(b"a", 8u32);
        assert_eq!(a.get(), &8);

        assert!(!env::storage_has_key(b"a"));
        a.flush();
        assert_eq!(u32::try_from_slice(&env::storage_read(b"a").unwrap()).unwrap(), 8);

        a.set(42);

        // Value in storage will still be 8 until the value is flushed
        assert_eq!(u32::try_from_slice(&env::storage_read(b"a").unwrap()).unwrap(), 8);
        assert_eq!(*a, 42);

        *a = 30;
        let serialized = to_vec(&a).unwrap();
        drop(a);
        assert_eq!(u32::try_from_slice(&env::storage_read(b"a").unwrap()).unwrap(), 30);

        let lazy_loaded = Lazy::<u32>::try_from_slice(&serialized).unwrap();
        assert!(lazy_loaded.cache.get().is_none());

        let b = Lazy::new(b"b", 30);
        assert!(!env::storage_has_key(b"b"));

        // A value that is not stored in storage yet and one that has not been loaded yet can
        // be checked for equality.
        assert_eq!(lazy_loaded, b);
    }

    #[test]
    pub fn test_debug() {
        let mut lazy = Lazy::new(b"m", 8u8);
        if cfg!(feature = "expensive-debug") {
            assert_eq!(format!("{:?}", lazy), "8");
        } else {
            assert_eq!(format!("{:?}", lazy), "Lazy { storage_key: [109], cache: Some(CacheEntry { value: Some(8), state: Modified }) }");
        }

        lazy.flush();
        if !cfg!(feature = "expensive-debug") {
            assert_eq!(format!("{:?}", lazy), "Lazy { storage_key: [109], cache: Some(CacheEntry { value: Some(8), state: Cached }) }");
        }

        // Serialize and deserialize to simulate storing and loading.
        let serialized = borsh::to_vec(&lazy).unwrap();
        drop(lazy);
        let lazy = Lazy::<u8>::try_from_slice(&serialized).unwrap();
        if cfg!(feature = "expensive-debug") {
            assert_eq!(format!("{:?}", lazy), "8");
        } else {
            assert_eq!(format!("{:?}", lazy), "Lazy { storage_key: [109], cache: None }");
        }
    }
}
