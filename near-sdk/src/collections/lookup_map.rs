//! A persistent map without iterators. Unlike `near_sdk::collections::UnorderedMap` this map
//! doesn't store keys and values separately in vectors, so it can't iterate over keys. But it
//! makes this map more efficient in the number of reads and writes.
use std::marker::PhantomData;

use borsh::{to_vec, BorshDeserialize, BorshSerialize};

use crate::collections::append_slice;
use crate::{env, errors, IntoStorageKey};
use near_sdk_macros::near;

/// An non-iterable implementation of a map that stores its content directly on the trie.
#[near(inside_nearsdk)]
pub struct LookupMap<K, V> {
    key_prefix: Vec<u8>,
    #[borsh(skip)]
    el: PhantomData<(K, V)>,
}

impl<K, V> LookupMap<K, V> {
    /// Create a new map. Use `key_prefix` as a unique prefix for keys.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::collections::LookupMap;
    /// let mut map: LookupMap<String, String> = LookupMap::new(b"m");
    /// ```
    pub fn new<S>(key_prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self { key_prefix: key_prefix.into_storage_key(), el: PhantomData }
    }

    fn raw_key_to_storage_key(&self, raw_key: &[u8]) -> Vec<u8> {
        append_slice(&self.key_prefix, raw_key)
    }

    /// Returns `true` if the serialized key is present in the map.
    fn contains_key_raw(&self, key_raw: &[u8]) -> bool {
        let storage_key = self.raw_key_to_storage_key(key_raw);
        env::storage_has_key(&storage_key)
    }

    /// Returns the serialized value corresponding to the serialized key.
    fn get_raw(&self, key_raw: &[u8]) -> Option<Vec<u8>> {
        let storage_key = self.raw_key_to_storage_key(key_raw);
        env::storage_read(&storage_key)
    }

    /// Inserts a serialized key-value pair into the map.
    /// If the map did not have this key present, `None` is returned. Otherwise returns
    /// a serialized value. Note, the keys that have the same hash value are undistinguished by
    /// the implementation.
    pub fn insert_raw(&mut self, key_raw: &[u8], value_raw: &[u8]) -> Option<Vec<u8>> {
        let storage_key = self.raw_key_to_storage_key(key_raw);
        if env::storage_write(&storage_key, value_raw) {
            Some(env::storage_get_evicted().unwrap())
        } else {
            None
        }
    }

    /// Removes a serialized key from the map, returning the serialized value at the key if the key
    /// was previously in the map.
    pub fn remove_raw(&mut self, key_raw: &[u8]) -> Option<Vec<u8>> {
        let storage_key = self.raw_key_to_storage_key(key_raw);
        if env::storage_remove(&storage_key) {
            Some(env::storage_get_evicted().unwrap())
        } else {
            None
        }
    }
}

impl<K, V> LookupMap<K, V>
where
    K: BorshSerialize,
    V: BorshSerialize + BorshDeserialize,
{
    fn serialize_key(key: &K) -> Vec<u8> {
        match to_vec(key) {
            Ok(x) => x,
            Err(_) => env::panic_err(errors::BorshSerializeError::new("key").into()),
        }
    }

    fn deserialize_value(raw_value: &[u8]) -> V {
        match V::try_from_slice(raw_value) {
            Ok(x) => x,
            Err(_) => env::panic_err(errors::BorshDeserializeError::new("value").into()),
        }
    }

    fn serialize_value(value: &V) -> Vec<u8> {
        match to_vec(value) {
            Ok(x) => x,
            Err(_) => env::panic_err(errors::BorshSerializeError::new("value").into()),
        }
    }

    /// Returns true if the map contains a given key.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::collections::LookupMap;
    ///
    /// let mut map: LookupMap<String, String> = LookupMap::new(b"m");
    /// assert_eq!(map.contains_key(&"Toyota".into()), false);
    ///
    /// map.insert(&"Toyota".into(), &"Camry".into());
    /// assert_eq!(map.contains_key(&"Toyota".into()), true);
    /// ```
    pub fn contains_key(&self, key: &K) -> bool {
        self.contains_key_raw(&Self::serialize_key(key))
    }

    /// Returns the value corresponding to the key.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::collections::LookupMap;
    ///
    /// let mut map: LookupMap<String, String> = LookupMap::new(b"m");
    /// assert_eq!(map.get(&"Toyota".into()), None);
    ///
    /// map.insert(&"Toyota".into(), &"Camry".into());
    /// assert_eq!(map.get(&"Toyota".into()), Some("Camry".into()));
    /// ```
    pub fn get(&self, key: &K) -> Option<V> {
        self.get_raw(&Self::serialize_key(key)).map(|value_raw| Self::deserialize_value(&value_raw))
    }

    /// Removes a key from the map, returning the value at the key if the key was previously in the
    /// map.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::collections::LookupMap;
    ///
    /// let mut map: LookupMap<String, String> = LookupMap::new(b"m");
    /// assert_eq!(map.remove(&"Toyota".into()), None);
    ///
    /// map.insert(&"Toyota".into(), &"Camry".into());
    /// assert_eq!(map.remove(&"Toyota".into()), Some("Camry".into()));
    /// ```
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.remove_raw(&Self::serialize_key(key))
            .map(|value_raw| Self::deserialize_value(&value_raw))
    }

    /// Inserts a key-value pair into the map.
    /// If the map did not have this key present, `None` is returned. Otherwise returns
    /// a value. Note, the keys that have the same hash value are undistinguished by
    /// the implementation.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::collections::LookupMap;
    ///
    /// let mut map: LookupMap<String, String> = LookupMap::new(b"m");
    /// assert_eq!(map.insert(&"Toyota".into(), &"Camry".into()), None);
    /// assert_eq!(map.insert(&"Toyota".into(), &"Corolla".into()), Some("Camry".into()));
    /// ```
    pub fn insert(&mut self, key: &K, value: &V) -> Option<V> {
        self.insert_raw(&Self::serialize_key(key), &Self::serialize_value(value))
            .map(|value_raw| Self::deserialize_value(&value_raw))
    }

    /// Inserts all new key-values from the iterator and replaces values with existing keys
    /// with new values returned from the iterator.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::collections::LookupMap;
    ///
    /// let mut extendee: LookupMap<String, String> = LookupMap::new(b"m");
    /// let mut source = vec![];
    ///
    /// source.push(("Toyota".into(), "Camry".into()));
    /// source.push(("Nissan".into(), "Almera".into()));
    /// source.push(("Ford".into(), "Mustang".into()));
    /// source.push(("Chevrolet".into(), "Camaro".into()));
    /// extendee.extend(source.into_iter());
    /// ```
    pub fn extend<IT: IntoIterator<Item = (K, V)>>(&mut self, iter: IT) {
        for (el_key, el_value) in iter {
            self.insert(&el_key, &el_value);
        }
    }
}

impl<K, V> std::fmt::Debug for LookupMap<K, V>
where
    K: std::fmt::Debug + BorshSerialize,
    V: std::fmt::Debug + BorshSerialize + BorshDeserialize,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LookupMap").field("key_prefix", &self.key_prefix).finish()
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use crate::collections::LookupMap;
    use rand::seq::SliceRandom;
    use rand::{Rng, SeedableRng};
    use std::collections::HashMap;

    #[test]
    pub fn test_insert_one() {
        let mut map = LookupMap::new(b"m");
        assert_eq!(None, map.insert(&1, &2));
        assert_eq!(2, map.insert(&1, &3).unwrap());
    }

    #[test]
    pub fn test_insert() {
        let mut map = LookupMap::new(b"m");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(0);
        for _ in 0..500 {
            let key = rng.gen::<u64>();
            let value = rng.gen::<u64>();
            map.insert(&key, &value);
        }
    }

    #[test]
    pub fn test_insert_has_key() {
        let mut map = LookupMap::new(b"m");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(0);
        let mut key_to_value = HashMap::new();
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            let value = rng.gen::<u64>();
            map.insert(&key, &value);
            key_to_value.insert(key, value);
        }
        // Non existing
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            assert_eq!(map.contains_key(&key), key_to_value.contains_key(&key));
        }
        // Existing
        for (key, _) in key_to_value.iter() {
            assert!(map.contains_key(key));
        }
    }

    #[test]
    pub fn test_insert_remove() {
        let mut map = LookupMap::new(b"m");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(1);
        let mut keys = vec![];
        let mut key_to_value = HashMap::new();
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            let value = rng.gen::<u64>();
            keys.push(key);
            key_to_value.insert(key, value);
            map.insert(&key, &value);
        }
        keys.shuffle(&mut rng);
        for key in keys {
            let actual = map.remove(&key).unwrap();
            assert_eq!(actual, key_to_value[&key]);
        }
    }

    #[test]
    pub fn test_remove_last_reinsert() {
        let mut map = LookupMap::new(b"m");
        let key1 = 1u64;
        let value1 = 2u64;
        map.insert(&key1, &value1);
        let key2 = 3u64;
        let value2 = 4u64;
        map.insert(&key2, &value2);

        let actual_value2 = map.remove(&key2).unwrap();
        assert_eq!(actual_value2, value2);

        let actual_insert_value2 = map.insert(&key2, &value2);
        assert_eq!(actual_insert_value2, None);
    }

    #[test]
    pub fn test_insert_override_remove() {
        let mut map = LookupMap::new(b"m");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(2);
        let mut keys = vec![];
        let mut key_to_value = HashMap::new();
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            let value = rng.gen::<u64>();
            keys.push(key);
            key_to_value.insert(key, value);
            map.insert(&key, &value);
        }
        keys.shuffle(&mut rng);
        for key in &keys {
            let value = rng.gen::<u64>();
            let actual = map.insert(key, &value).unwrap();
            assert_eq!(actual, key_to_value[key]);
            key_to_value.insert(*key, value);
        }
        keys.shuffle(&mut rng);
        for key in keys {
            let actual = map.remove(&key).unwrap();
            assert_eq!(actual, key_to_value[&key]);
        }
    }

    #[test]
    pub fn test_get_non_existent() {
        let mut map = LookupMap::new(b"m");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(3);
        let mut key_to_value = HashMap::new();
        for _ in 0..500 {
            let key = rng.gen::<u64>() % 20_000;
            let value = rng.gen::<u64>();
            key_to_value.insert(key, value);
            map.insert(&key, &value);
        }
        for _ in 0..500 {
            let key = rng.gen::<u64>() % 20_000;
            assert_eq!(map.get(&key), key_to_value.get(&key).cloned());
        }
    }

    #[test]
    pub fn test_extend() {
        let mut map = LookupMap::new(b"m");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(4);
        let mut key_to_value = HashMap::new();
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            let value = rng.gen::<u64>();
            key_to_value.insert(key, value);
            map.insert(&key, &value);
        }
        for _ in 0..10 {
            let mut tmp = vec![];
            for _ in 0..=(rng.gen::<u64>() % 20 + 1) {
                let key = rng.gen::<u64>();
                let value = rng.gen::<u64>();
                tmp.push((key, value));
            }
            key_to_value.extend(tmp.iter().cloned());
            map.extend(tmp.iter().cloned());
        }

        for (key, value) in key_to_value {
            assert_eq!(map.get(&key).unwrap(), value);
        }
    }

    #[test]
    fn test_debug() {
        let map: LookupMap<u64, u64> = LookupMap::new(b"m");

        assert_eq!(
            format!("{:?}", map),
            format!("LookupMap {{ key_prefix: {:?} }}", map.key_prefix)
        );
    }
}
