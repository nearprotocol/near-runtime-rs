//! A persistent set without iterators. Unlike `near_sdk::collections::LookupSet` this set
//! doesn't store values separately in a vector, so it can't iterate over the values. But it
//! makes this implementation more efficient in the number of reads and writes.
use std::marker::PhantomData;

use borsh::{to_vec, BorshSerialize};
use near_sdk_macros::near;

use crate::collections::append_slice;
use crate::{env, errors, IntoStorageKey};

/// A non-iterable implementation of a set that stores its content directly on the storage trie.
///
/// This set stores the values under a hash of the set's `prefix` and [`BorshSerialize`] of the
/// value.
#[near(inside_nearsdk)]
pub struct LookupSet<T> {
    element_prefix: Vec<u8>,
    #[borsh(skip)]
    el: PhantomData<T>,
}

impl<T> LookupSet<T> {
    /// Create a new map. Use `element_prefix` as a unique prefix for trie keys.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::collections::LookupSet;
    /// let mut set: LookupSet<u32> = LookupSet::new(b"s");
    /// ```
    pub fn new<S>(element_prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self { element_prefix: element_prefix.into_storage_key(), el: PhantomData }
    }

    fn raw_element_to_storage_key(&self, element_raw: &[u8]) -> Vec<u8> {
        append_slice(&self.element_prefix, element_raw)
    }

    /// Returns `true` if the serialized key is present in the map.
    fn contains_raw(&self, element_raw: &[u8]) -> bool {
        let storage_key = self.raw_element_to_storage_key(element_raw);
        env::storage_has_key(&storage_key)
    }

    /// Inserts a serialized element into the set.
    /// If the set did not have this value present, `true` is returned.
    /// If the set did have this value present, `false` is returned.
    pub fn insert_raw(&mut self, element_raw: &[u8]) -> bool {
        let storage_key = self.raw_element_to_storage_key(element_raw);
        !env::storage_write(&storage_key, b"")
    }

    /// Removes a serialized element from the set.
    /// Returns true if the element was present in the set.
    pub fn remove_raw(&mut self, element_raw: &[u8]) -> bool {
        let storage_key = self.raw_element_to_storage_key(element_raw);
        env::storage_remove(&storage_key)
    }
}

impl<T> LookupSet<T>
where
    T: BorshSerialize,
{
    fn serialize_element(element: &T) -> Vec<u8> {
        match to_vec(element) {
            Ok(x) => x,
            Err(_) => env::panic_err(errors::BorshSerializeError::new("element").into()),
        }
    }

    /// Returns true if the set contains an element.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::collections::LookupSet;
    ///
    /// let mut set: LookupSet<String> = LookupSet::new(b"s");
    /// assert_eq!(set.contains(&"Element".into()), false);
    ///
    /// set.insert(&"Element".into());
    /// assert_eq!(set.contains(&"Element".into()), true);
    /// ```
    pub fn contains(&self, element: &T) -> bool {
        self.contains_raw(&Self::serialize_element(element))
    }

    /// Removes a value from the set. Returns whether the value was present in the set.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::collections::LookupSet;
    ///
    /// let mut set: LookupSet<String> = LookupSet::new(b"s");
    /// assert_eq!(set.remove(&"Element".into()), false);
    ///
    /// set.insert(&"Element".into());
    /// assert_eq!(set.remove(&"Element".into()), true);
    /// ```
    pub fn remove(&mut self, element: &T) -> bool {
        self.remove_raw(&Self::serialize_element(element))
    }

    /// Adds a value to the set.
    /// If the set did not have this value present, `true` is returned.
    /// If the set did have this value present, `false` is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::collections::LookupSet;
    ///
    /// let mut set: LookupSet<String> = LookupSet::new(b"s");
    /// assert_eq!(set.insert(&"Element".into()), true);
    /// assert_eq!(set.insert(&"Element".into()), false);
    /// ```
    pub fn insert(&mut self, element: &T) -> bool {
        self.insert_raw(&Self::serialize_element(element))
    }

    pub fn extend<IT: IntoIterator<Item = T>>(&mut self, iter: IT) {
        for el in iter {
            self.insert(&el);
        }
    }
}

impl<T> std::fmt::Debug for LookupSet<T>
where
    T: std::fmt::Debug + BorshSerialize,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LookupSet").field("element_prefix", &self.element_prefix).finish()
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use crate::collections::LookupSet;
    use rand::seq::SliceRandom;
    use rand::{Rng, SeedableRng};
    use std::collections::HashSet;

    #[test]
    pub fn test_insert_one() {
        let mut map = LookupSet::new(b"m");
        assert!(map.insert(&1));
        assert!(!map.insert(&1));
    }

    #[test]
    pub fn test_insert() {
        let mut set = LookupSet::new(b"s");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(0);
        for _ in 0..500 {
            let key = rng.gen::<u64>();
            set.insert(&key);
        }
    }

    #[test]
    pub fn test_insert_remove() {
        let mut set = LookupSet::new(b"s");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(1);
        let mut keys = vec![];
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            keys.push(key);
            set.insert(&key);
        }
        keys.shuffle(&mut rng);
        for key in keys {
            assert!(set.remove(&key));
        }
    }

    #[test]
    pub fn test_remove_last_reinsert() {
        let mut set = LookupSet::new(b"s");
        let key1 = 1u64;
        set.insert(&key1);
        let key2 = 2u64;
        set.insert(&key2);

        let actual = set.remove(&key2);
        assert!(actual);

        let actual_reinsert = set.insert(&key2);
        assert!(actual_reinsert);
    }

    #[test]
    pub fn test_insert_override_remove() {
        let mut set = LookupSet::new(b"s");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(2);
        let mut keys = vec![];
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            keys.push(key);
            set.insert(&key);
        }
        keys.shuffle(&mut rng);
        for key in &keys {
            assert!(!set.insert(key));
        }
        keys.shuffle(&mut rng);
        for key in keys {
            assert!(set.remove(&key));
        }
    }

    #[test]
    pub fn test_contains_non_existent() {
        let mut set = LookupSet::new(b"s");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(3);
        let mut set_tmp = HashSet::new();
        for _ in 0..500 {
            let key = rng.gen::<u64>() % 20_000;
            set_tmp.insert(key);
            set.insert(&key);
        }
        for _ in 0..500 {
            let key = rng.gen::<u64>() % 20_000;
            assert_eq!(set.contains(&key), set_tmp.contains(&key));
        }
    }

    #[test]
    pub fn test_extend() {
        let mut set = LookupSet::new(b"s");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(4);
        let mut keys = HashSet::new();
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            keys.insert(key);
            set.insert(&key);
        }
        for _ in 0..10 {
            let mut tmp = vec![];
            for _ in 0..=(rng.gen::<u64>() % 20 + 1) {
                let key = rng.gen::<u64>();
                tmp.push(key);
            }
            keys.extend(tmp.iter().cloned());
            set.extend(tmp.iter().cloned());
        }

        for key in keys {
            assert!(set.contains(&key));
        }
    }

    #[test]
    fn test_debug() {
        let set: LookupSet<u64> = LookupSet::new(b"m");

        assert_eq!(
            format!("{:?}", set),
            format!("LookupSet {{ element_prefix: {:?} }}", set.element_prefix)
        );
    }
}
