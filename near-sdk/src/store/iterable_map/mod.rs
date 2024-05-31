// This suppresses the depreciation warnings for uses of UnorderedSet in this module
#![allow(deprecated)]

mod entry;
mod impls;
mod iter;

use std::borrow::Borrow;
use std::{fmt, mem};

use borsh::{BorshDeserialize, BorshSerialize};

use near_sdk_macros::near;

use crate::store::key::{Sha256, ToKey};
use crate::{env, errors, IntoStorageKey};

use crate::store::Vector;
pub use entry::{Entry, OccupiedEntry, VacantEntry};

pub use self::iter::{Drain, Iter, IterMut, Keys, Values, ValuesMut};
use super::LookupMap;

/// A lazily loaded storage map that stores its content directly on the storage trie.
/// This structure is similar to [`near_sdk::store::LookupMap`](crate::store::LookupMap), except
/// that it stores the keys so that [`IterableMap`] can be iterable.
///
/// This map stores the values under a hash of the map's `prefix` and [`BorshSerialize`] of the key
/// using the map's [`ToKey`] implementation.
///
/// The default hash function for [`IterableMap`] is [`Sha256`] which uses a syscall
/// (or host function) built into the NEAR runtime to hash the key. To use a custom function,
/// use [`with_hasher`]. Alternative builtin hash functions can be found at
/// [`near_sdk::store::key`](crate::store::key).
///
///
/// # Examples
/// ```
/// use near_sdk::store::IterableMap;
///
/// // Initializes a map, the generic types can be inferred to `IterableMap<String, u8, Sha256>`
/// // The `b"a"` parameter is a prefix for the storage keys of this data structure.
/// let mut map = IterableMap::new(b"a");
///
/// map.insert("test".to_string(), 7u8);
/// assert!(map.contains_key("test"));
/// assert_eq!(map.get("test"), Some(&7u8));
///
/// let prev = std::mem::replace(map.get_mut("test").unwrap(), 5u8);
/// assert_eq!(prev, 7u8);
/// assert_eq!(map["test"], 5u8);
/// ```
///
/// [`IterableMap`] also implements an [`Entry API`](Self::entry), which allows
/// for more complex methods of getting, setting, updating and removing keys and
/// their values:
///
/// ```
/// use near_sdk::store::IterableMap;
///
/// // type inference lets us omit an explicit type signature (which
/// // would be `IterableMap<String, u8>` in this example).
/// let mut player_stats = IterableMap::new(b"m");
///
/// fn random_stat_buff() -> u8 {
///     // could actually return some random value here - let's just return
///     // some fixed value for now
///     42
/// }
///
/// // insert a key only if it doesn't already exist
/// player_stats.entry("health".to_string()).or_insert(100);
///
/// // insert a key using a function that provides a new value only if it
/// // doesn't already exist
/// player_stats.entry("defence".to_string()).or_insert_with(random_stat_buff);
///
/// // update a key, guarding against the key possibly not being set
/// let stat = player_stats.entry("attack".to_string()).or_insert(100);
/// *stat += random_stat_buff();
/// ```
///
/// [`with_hasher`]: Self::with_hasher
#[derive(BorshDeserialize, BorshSerialize)]
pub struct IterableMap<K, V, H = Sha256>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    // NOTE: It's important that the `keys` collection  is one that's optimized for iteration, e.g.
    // not skipping empty/unoccupied entries white trying to get to the next element.
    // See https://github.com/near/near-sdk-rs/issues/1134 to understand the difference between
    // `store::UnorderedMap` and `store::IterableMap`.

    // ser/de is independent of `K` ser/de, `BorshSerialize`/`BorshDeserialize` bounds removed
    #[borsh(bound(serialize = "", deserialize = ""))]
    keys: Vector<K>,
    // ser/de is independent of `K`, `V`, `H` ser/de, `BorshSerialize`/`BorshDeserialize` bounds removed
    #[borsh(bound(serialize = "", deserialize = ""))]
    values: LookupMap<K, ValueAndIndex<V>, H>,
}

#[near(inside_nearsdk)]
struct ValueAndIndex<V> {
    value: V,
    key_index: u32,
}

impl<K, V, H> Drop for IterableMap<K, V, H>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    fn drop(&mut self) {
        self.flush()
    }
}

impl<K, V, H> fmt::Debug for IterableMap<K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + fmt::Debug,
    V: BorshSerialize,
    H: ToKey,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IterableMap")
            .field("keys", &self.keys)
            .field("values", &self.values)
            .finish()
    }
}

impl<K, V> IterableMap<K, V, Sha256>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
{
    /// Create a new iterable map. Use `prefix` as a unique prefix for keys.
    ///
    /// This prefix can be anything that implements [`IntoStorageKey`]. The prefix is used when
    /// storing and looking up values in storage to ensure no collisions with other collections.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map: IterableMap<String, u8> = IterableMap::new(b"b");
    /// ```
    #[inline]
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self::with_hasher(prefix)
    }
}

impl<K, V, H> IterableMap<K, V, H>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    /// Initialize a [`IterableMap`] with a custom hash function.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::{IterableMap, key::Keccak256};
    ///
    /// let map = IterableMap::<String, String, Keccak256>::with_hasher(b"m");
    /// ```
    pub fn with_hasher<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        let mut vec_key = prefix.into_storage_key();
        let map_key = [vec_key.as_slice(), b"m"].concat();
        vec_key.push(b'v');
        Self { keys: Vector::new(vec_key), values: LookupMap::with_hasher(map_key) }
    }

    /// Return the amount of elements inside of the map.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map: IterableMap<String, u8> = IterableMap::new(b"b");
    /// assert_eq!(map.len(), 0);
    /// map.insert("a".to_string(), 1);
    /// map.insert("b".to_string(), 2);
    /// assert_eq!(map.len(), 2);
    /// ```
    pub fn len(&self) -> u32 {
        self.keys.len()
    }

    /// Returns true if there are no elements inside of the map.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map: IterableMap<String, u8> = IterableMap::new(b"b");
    /// assert!(map.is_empty());
    /// map.insert("a".to_string(), 1);
    /// assert!(!map.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Clears the map, removing all key-value pairs. Keeps the allocated memory
    /// for reuse.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map: IterableMap<String, u8> = IterableMap::new(b"b");
    /// map.insert("a".to_string(), 1);
    ///
    /// map.clear();
    ///
    /// assert!(map.is_empty());
    /// ```
    pub fn clear(&mut self)
    where
        K: BorshDeserialize + Clone,
        V: BorshDeserialize,
    {
        for k in self.keys.drain(..) {
            // Set instead of remove to avoid loading the value from storage.
            self.values.set(k, None);
        }
    }

    /// An iterator visiting all key-value pairs in arbitrary order.
    /// The iterator element type is `(&'a K, &'a V)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map = IterableMap::new(b"m");
    /// map.insert("a".to_string(), 1);
    /// map.insert("b".to_string(), 2);
    /// map.insert("c".to_string(), 3);
    ///
    /// for (key, val) in map.iter() {
    ///     println!("key: {} val: {}", key, val);
    /// }
    /// ```
    pub fn iter(&self) -> Iter<K, V, H>
    where
        K: BorshDeserialize,
    {
        Iter::new(self)
    }

    /// An iterator visiting all key-value pairs in arbitrary order,
    /// with exclusive references to the values.
    /// The iterator element type is `(&'a K, &'a mut V)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map = IterableMap::new(b"m");
    /// map.insert("a".to_string(), 1);
    /// map.insert("b".to_string(), 2);
    /// map.insert("c".to_string(), 3);
    ///
    /// // Update all values
    /// for (_, val) in map.iter_mut() {
    ///     *val *= 2;
    /// }
    ///
    /// for (key, val) in &map {
    ///     println!("key: {} val: {}", key, val);
    /// }
    /// ```
    pub fn iter_mut(&mut self) -> IterMut<K, V, H>
    where
        K: BorshDeserialize,
    {
        IterMut::new(self)
    }

    /// An iterator visiting all keys in arbitrary order.
    /// The iterator element type is `&'a K`.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map = IterableMap::new(b"m");
    /// map.insert("a".to_string(), 1);
    /// map.insert("b".to_string(), 2);
    /// map.insert("c".to_string(), 3);
    ///
    /// for key in map.keys() {
    ///     println!("{}", key);
    /// }
    /// ```
    pub fn keys(&self) -> Keys<K>
    where
        K: BorshDeserialize,
    {
        Keys::new(self)
    }

    /// An iterator visiting all values in arbitrary order.
    /// The iterator element type is `&'a V`.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map = IterableMap::new(b"m");
    /// map.insert("a".to_string(), 1);
    /// map.insert("b".to_string(), 2);
    /// map.insert("c".to_string(), 3);
    ///
    /// for val in map.values() {
    ///     println!("{}", val);
    /// }
    /// ```
    pub fn values(&self) -> Values<K, V, H>
    where
        K: BorshDeserialize,
    {
        Values::new(self)
    }

    /// A mutable iterator visiting all values in arbitrary order.
    /// The iterator element type is `&'a mut V`.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map = IterableMap::new(b"m");
    /// map.insert("a".to_string(), 1);
    /// map.insert("b".to_string(), 2);
    /// map.insert("c".to_string(), 3);
    ///
    /// for val in map.values_mut() {
    ///     *val = *val + 10;
    /// }
    ///
    /// for val in map.values() {
    ///     println!("{}", val);
    /// }
    /// ```
    pub fn values_mut(&mut self) -> ValuesMut<K, V, H>
    where
        K: BorshDeserialize,
    {
        ValuesMut::new(self)
    }

    /// Clears the map, returning all key-value pairs as an iterator.
    ///
    /// This will clear all values, even if only some key/value pairs are yielded.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut a = IterableMap::new(b"m");
    /// a.insert(1, "a".to_string());
    /// a.insert(2, "b".to_string());
    ///
    /// for (k, v) in a.drain().take(1) {
    ///     assert!(k == 1 || k == 2);
    ///     assert!(&v == "a" || &v == "b");
    /// }
    ///
    /// assert!(a.is_empty());
    /// ```
    pub fn drain(&mut self) -> Drain<K, V, H>
    where
        K: BorshDeserialize,
    {
        Drain::new(self)
    }
}

impl<K, V, H> IterableMap<K, V, H>
where
    K: BorshSerialize + Ord + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: ToKey,
{
    /// Returns a reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`BorshSerialize`] and [`ToOwned<Owned = K>`](ToOwned) on the borrowed form *must* match
    /// those for the key type.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map: IterableMap<String, u8> = IterableMap::new(b"b");
    /// assert!(map.insert("test".to_string(), 5u8).is_none());
    /// assert_eq!(map.get("test"), Some(&5));
    /// ```
    pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: BorshSerialize + ToOwned<Owned = K>,
    {
        self.values.get(k).map(|v| &v.value)
    }

    /// Returns a mutable reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`BorshSerialize`] and [`ToOwned<Owned = K>`](ToOwned) on the borrowed form *must* match
    /// those for the key type.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map: IterableMap<String, u8> = IterableMap::new(b"b");
    /// assert!(map.insert("test".to_string(), 5u8).is_none());
    ///
    /// *map.get_mut("test").unwrap() = 6;
    /// assert_eq!(map["test"], 6);
    /// ```
    pub fn get_mut<Q: ?Sized>(&mut self, k: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: BorshSerialize + ToOwned<Owned = K>,
    {
        self.values.get_mut(k).map(|v| &mut v.value)
    }

    /// Inserts a key-value pair into the map.
    ///
    /// If the map did not have this key present, [`None`] is returned.
    ///
    /// If the map did have this key present, the value is updated, and the old
    /// value is returned. The key is not updated, though; this matters for
    /// types that can be `==` without being identical.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map: IterableMap<String, u8> = IterableMap::new(b"b");
    /// assert!(map.is_empty());
    ///
    /// map.insert("a".to_string(), 1);
    ///
    /// assert!(!map.is_empty());
    /// assert_eq!(map.values().collect::<Vec<_>>(), [&1]);
    /// ```
    pub fn insert(&mut self, k: K, value: V) -> Option<V>
    where
        K: Clone + BorshDeserialize,
    {
        // Check if value is in map to replace first
        let entry = self.values.get_mut_inner(&k);
        if let Some(existing) = entry.value_mut() {
            return Some(mem::replace(&mut existing.value, value));
        }

        // At this point, we know that the key-value doesn't exist in the map, add key to bucket.
        self.keys.push(k);
        let key_index = self.keys.len() - 1;
        entry.replace(Some(ValueAndIndex { value, key_index }));
        None
    }

    /// Returns `true` if the map contains a value for the specified key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`BorshSerialize`] and [`ToOwned<Owned = K>`](ToOwned) on the borrowed form *must* match
    /// those for the key type.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map: IterableMap<String, u8> = IterableMap::new(b"b");
    /// map.insert("test".to_string(), 7u8);
    ///
    /// assert!(map.contains_key("test"));
    /// ```
    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: BorshSerialize + ToOwned<Owned = K> + Ord,
    {
        self.values.contains_key(k)
    }

    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`BorshSerialize`] and [`ToOwned<Owned = K>`](ToOwned) on the borrowed form *must* match
    /// those for the key type.
    ///
    /// # Performance
    ///
    /// When elements are removed, the underlying vector of keys is rearranged by means of swapping
    /// an obsolete key with the last element in the list and deleting that. Note that that requires
    /// updating the `values` map due to the fact that it holds `keys` vector indices.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map: IterableMap<String, u8> = IterableMap::new(b"b");
    /// map.insert("test".to_string(), 7u8);
    /// assert_eq!(map.len(), 1);
    ///
    /// map.remove("test");
    ///
    /// assert_eq!(map.len(), 0);
    /// ```
    pub fn remove<Q: ?Sized>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q> + BorshDeserialize,
        Q: BorshSerialize + ToOwned<Owned = K>,
    {
        self.remove_entry(k).map(|(_, v)| v)
    }

    /// Removes a key from the map, returning the stored key and value if the
    /// key was previously in the map.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`BorshSerialize`] and [`ToOwned<Owned = K>`](ToOwned) on the borrowed form *must* match
    /// those for the key type.
    ///
    /// # Performance
    ///
    /// When elements are removed, the underlying vector of keys is rearranged by means of swapping
    /// an obsolete key with the last element in the list and deleting that. Note that that requires
    /// updating the `values` map due to the fact that it holds `keys` vector indices.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut map = IterableMap::new(b"m");
    /// map.insert(1, "a".to_string());
    /// assert_eq!(map.remove(&1), Some("a".to_string()));
    /// assert_eq!(map.remove(&1), None);
    /// ```
    pub fn remove_entry<Q: ?Sized>(&mut self, k: &Q) -> Option<(K, V)>
    where
        K: BorshDeserialize + Clone,
        Q: BorshSerialize + ToOwned<Owned = K>,
    {
        // Remove value
        let old_value = self.values.remove(&k.to_owned())?;

        // Remove key with index if value exists
        let last_index = self.keys.len() - 1;
        let key = self.keys.swap_remove(old_value.key_index);

        Self::remove_entry_helper(&self.keys, &mut self.values, old_value.key_index, last_index);

        // Return removed value
        Some((key, old_value.value))
    }

    fn remove_entry_helper(
        keys: &Vector<K>,
        values: &mut LookupMap<K, ValueAndIndex<V>, H>,
        key_index: u32,
        last_index: u32,
    ) where
        K: BorshDeserialize + Clone,
    {
        match key_index {
            // If it's the last/only element - do nothing.
            x if x == last_index => {}
            // Otherwise update it's index.
            _ => {
                let swapped_key = keys.get(key_index).unwrap_or_else(|| {
                    env::panic_err(errors::InconsistentCollectionState::new().into())
                });
                let value = values.get_mut(swapped_key).unwrap_or_else(|| {
                    env::panic_err(errors::InconsistentCollectionState::new().into())
                });
                value.key_index = key_index;
            }
        }
    }

    /// Gets the given key's corresponding entry in the map for in-place manipulation.
    ///
    /// # Performance
    /// Note that due to the fact that we need to potentially re-arrange `keys` and update `values`,
    /// `Entry` API actually operates on those two collections as opposed to an actual `Entry`
    /// ```
    /// use near_sdk::store::IterableMap;
    ///
    /// let mut count = IterableMap::new(b"m");
    ///
    /// for ch in [7, 2, 4, 7, 4, 1, 7] {
    ///     let counter = count.entry(ch).or_insert(0);
    ///     *counter += 1;
    /// }
    ///
    /// assert_eq!(count[&4], 2);
    /// assert_eq!(count[&7], 3);
    /// assert_eq!(count[&1], 1);
    /// assert_eq!(count.get(&8), None);
    /// ```
    pub fn entry(&mut self, key: K) -> Entry<K, V, H>
    where
        K: Clone,
    {
        Entry::new(key, &mut self.keys, &mut self.values)
    }
}

impl<K, V, H> IterableMap<K, V, H>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    /// Flushes the intermediate values of the map before this is called when the structure is
    /// [`Drop`]ed. This will write all modified values to storage but keep all cached values
    /// in memory.
    pub fn flush(&mut self) {
        self.keys.flush();
        self.values.flush();
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::IterableMap;
    use crate::test_utils::test_env::setup_free;
    use arbitrary::{Arbitrary, Unstructured};
    use borsh::{to_vec, BorshDeserialize};
    use rand::RngCore;
    use rand::SeedableRng;
    use std::collections::HashMap;

    #[test]
    fn basic_functionality() {
        let mut map = IterableMap::new(b"b");
        assert!(map.is_empty());
        assert!(map.insert("test".to_string(), 5u8).is_none());
        assert_eq!(map.get("test"), Some(&5));
        assert_eq!(map.len(), 1);

        *map.get_mut("test").unwrap() = 6;
        assert_eq!(map["test"], 6);

        assert_eq!(map.remove("test"), Some(6));
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn entry_api() {
        let mut map = IterableMap::new(b"b");
        {
            let test_entry = map.entry("test".to_string());
            assert_eq!(test_entry.key(), "test");
            let entry_ref = test_entry.or_insert(8u8);
            *entry_ref += 1;
        }
        assert_eq!(map["test"], 9);

        // Try getting entry of filled value
        let value = map.entry("test".to_string()).and_modify(|v| *v += 3).or_default();
        assert_eq!(*value, 12);
    }

    #[test]
    fn map_iterator() {
        let mut map = IterableMap::new(b"b");

        map.insert(0u8, 0u8);
        map.insert(1, 1);
        map.insert(2, 2);
        map.insert(3, 3);
        map.remove(&1);

        let iter = map.iter();
        assert_eq!(iter.len(), 3);
        assert_eq!(iter.collect::<Vec<_>>(), [(&0, &0), (&3, &3), (&2, &2)]);

        let iter = map.iter_mut().rev();
        assert_eq!(iter.collect::<Vec<_>>(), [(&2, &mut 2), (&3, &mut 3), (&0, &mut 0)]);

        let mut iter = map.iter();
        assert_eq!(iter.nth(2), Some((&2, &2)));
        // Check fused iterator assumption that each following one will be None
        assert_eq!(iter.next(), None);

        // Double all values
        map.values_mut().for_each(|v| {
            *v *= 2;
        });
        assert_eq!(map.values().collect::<Vec<_>>(), [&0, &6, &4]);

        // Collect all keys
        assert_eq!(map.keys().collect::<Vec<_>>(), [&0, &3, &2]);
    }

    #[derive(Arbitrary, Debug)]
    enum Op {
        Insert(u8, u8),
        Remove(u8),
        Flush,
        Restore,
        Get(u8),
    }

    #[test]
    fn arbitrary() {
        setup_free();

        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(0);
        let mut buf = vec![0; 4096];
        for _ in 0..512 {
            // Clear storage in-between runs
            crate::mock::with_mocked_blockchain(|b| b.take_storage());
            rng.fill_bytes(&mut buf);

            let mut um = IterableMap::new(b"l");
            let mut hm = HashMap::new();
            let u = Unstructured::new(&buf);
            if let Ok(ops) = Vec::<Op>::arbitrary_take_rest(u) {
                for op in ops {
                    match op {
                        Op::Insert(k, v) => {
                            let r1 = um.insert(k, v);
                            let r2 = hm.insert(k, v);
                            assert_eq!(r1, r2)
                        }
                        Op::Remove(k) => {
                            let r1 = um.remove(&k);
                            let r2 = hm.remove(&k);
                            assert_eq!(r1, r2)
                        }
                        Op::Flush => {
                            um.flush();
                        }
                        Op::Restore => {
                            let serialized = to_vec(&um).unwrap();
                            um = IterableMap::deserialize(&mut serialized.as_slice()).unwrap();
                        }
                        Op::Get(k) => {
                            let r1 = um.get(&k);
                            let r2 = hm.get(&k);
                            assert_eq!(r1, r2)
                        }
                    }
                }
            }
        }
    }
}

// Hashbrown-like tests.
#[cfg(test)]
mod test_map {
    use super::Entry::{Occupied, Vacant};
    use crate::store::IterableMap;
    use borsh::{BorshDeserialize, BorshSerialize};
    use rand::{rngs::SmallRng, Rng, SeedableRng};
    use std::cell::RefCell;
    use std::usize;
    use std::vec::Vec;

    #[test]
    fn test_insert() {
        let mut m = IterableMap::new(b"b");
        assert_eq!(m.len(), 0);
        assert!(m.insert(1, 2).is_none());
        assert_eq!(m.len(), 1);
        assert!(m.insert(2, 4).is_none());
        assert_eq!(m.len(), 2);
        assert_eq!(*m.get(&1).unwrap(), 2);
        assert_eq!(*m.get(&2).unwrap(), 4);
    }

    thread_local! { static DROP_VECTOR: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) }}

    #[derive(Hash, PartialEq, Eq, BorshSerialize, BorshDeserialize, PartialOrd, Ord)]
    struct Droppable {
        k: usize,
    }

    impl Droppable {
        fn new(k: usize) -> Droppable {
            DROP_VECTOR.with(|slot| {
                slot.borrow_mut()[k] += 1;
            });

            Droppable { k }
        }
    }

    impl Drop for Droppable {
        fn drop(&mut self) {
            DROP_VECTOR.with(|slot| {
                slot.borrow_mut()[self.k] -= 1;
            });
        }
    }

    impl Clone for Droppable {
        fn clone(&self) -> Self {
            Droppable::new(self.k)
        }
    }

    #[test]
    fn test_drops() {
        DROP_VECTOR.with(|slot| {
            *slot.borrow_mut() = vec![0; 200];
        });

        {
            let mut m = IterableMap::new(b"b");

            DROP_VECTOR.with(|v| {
                for i in 0..200 {
                    assert_eq!(v.borrow()[i], 0);
                }
            });

            for i in 0..100 {
                let d1 = Droppable::new(i);
                let d2 = Droppable::new(i + 100);
                m.insert(d1, d2);
            }

            DROP_VECTOR.with(|v| {
                for i in 0..100 {
                    assert_eq!(v.borrow()[i], 2);
                }
            });

            for i in 0..50 {
                let k = Droppable::new(i);
                let v = m.remove(&k);

                assert!(v.is_some());

                DROP_VECTOR.with(|v| {
                    assert_eq!(v.borrow()[i], 2);
                    assert_eq!(v.borrow()[i + 100], 1);
                });
            }

            DROP_VECTOR.with(|v| {
                for i in 0..50 {
                    assert_eq!(v.borrow()[i], 1);
                    assert_eq!(v.borrow()[i + 100], 0);
                }

                for i in 50..100 {
                    assert_eq!(v.borrow()[i], 2);
                    assert_eq!(v.borrow()[i + 100], 1);
                }
            });
        }

        DROP_VECTOR.with(|v| {
            for i in 0..200 {
                assert_eq!(v.borrow()[i], 0);
            }
        });
    }

    #[test]
    fn test_into_iter_drops() {
        DROP_VECTOR.with(|v| {
            *v.borrow_mut() = vec![0; 200];
        });

        let hm = {
            let mut hm = IterableMap::new(b"b");

            DROP_VECTOR.with(|v| {
                for i in 0..200 {
                    assert_eq!(v.borrow()[i], 0);
                }
            });

            for i in 0..100 {
                let d1 = Droppable::new(i);
                let d2 = Droppable::new(i + 100);
                hm.insert(d1, d2);
            }

            DROP_VECTOR.with(|v| {
                for i in 0..100 {
                    assert_eq!(v.borrow()[i], 2);
                }
                for i in 101..200 {
                    assert_eq!(v.borrow()[i], 1);
                }
            });

            hm
        };

        {
            let mut half = hm.into_iter().take(50);

            DROP_VECTOR.with(|v| {
                for i in 0..100 {
                    assert_eq!(v.borrow()[i], 2);
                }
                for i in 101..200 {
                    assert_eq!(v.borrow()[i], 1);
                }
            });

            #[allow(let_underscore_drop)] // kind-of a false positive
            for _ in half.by_ref() {}

            DROP_VECTOR.with(|v| {
                let nk = (0..100).filter(|&i| v.borrow()[i] == 2).count();

                let nv = (0..100).filter(|&i| v.borrow()[i + 100] == 1).count();

                assert_eq!(nk, 100);
                assert_eq!(nv, 100);
            });
        };

        drop(hm);

        DROP_VECTOR.with(|v| {
            for i in 0..200 {
                assert_eq!(v.borrow()[i], 0);
            }
        });
    }

    #[test]
    fn test_empty_remove() {
        let mut m: IterableMap<i32, bool> = IterableMap::new(b"b");
        assert_eq!(m.remove(&0), None);
    }

    #[test]
    fn test_empty_entry() {
        let mut m: IterableMap<i32, bool> = IterableMap::new(b"b");
        match m.entry(0) {
            Occupied(_) => panic!(),
            Vacant(_) => {}
        }
        assert!(*m.entry(0).or_insert(true));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn test_empty_iter() {
        let mut m: IterableMap<i32, bool> = IterableMap::new(b"b");
        assert_eq!(m.drain().next(), None);
        assert_eq!(m.keys().next(), None);
        assert_eq!(m.values().next(), None);
        assert_eq!(m.values_mut().next(), None);
        assert_eq!(m.iter().next(), None);
        assert_eq!(m.iter_mut().next(), None);
        assert_eq!(m.len(), 0);
        assert!(m.is_empty());
        assert_eq!(m.into_iter().next(), None);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // FIXME: takes too long
    fn test_lots_of_insertions() {
        let mut m = IterableMap::new(b"b");

        // Try this a few times to make sure we never screw up the IterableMap's
        // internal state.
        for _ in 0..10 {
            assert!(m.is_empty());

            for i in 1..1001 {
                assert!(m.insert(i, i).is_none());

                for j in 1..=i {
                    let r = m.get(&j);
                    assert_eq!(r, Some(&j));
                }

                for j in i + 1..1001 {
                    let r = m.get(&j);
                    assert_eq!(r, None);
                }
            }

            for i in 1001..2001 {
                assert!(!m.contains_key(&i));
            }

            // remove forwards
            for i in 1..1001 {
                assert!(m.remove(&i).is_some());

                for j in 1..=i {
                    assert!(!m.contains_key(&j));
                }

                for j in i + 1..1001 {
                    assert!(m.contains_key(&j));
                }
            }

            for i in 1..1001 {
                assert!(!m.contains_key(&i));
            }

            for i in 1..1001 {
                assert!(m.insert(i, i).is_none());
            }

            // remove backwards
            for i in (1..1001).rev() {
                assert!(m.remove(&i).is_some());

                for j in i..1001 {
                    assert!(!m.contains_key(&j));
                }

                for j in 1..i {
                    assert!(m.contains_key(&j));
                }
            }
        }
    }

    #[test]
    fn test_find_mut() {
        let mut m = IterableMap::new(b"b");
        assert!(m.insert(1, 12).is_none());
        assert!(m.insert(2, 8).is_none());
        assert!(m.insert(5, 14).is_none());
        let new = 100;
        match m.get_mut(&5) {
            None => panic!(),
            Some(x) => *x = new,
        }
        assert_eq!(m.get(&5), Some(&new));
    }

    #[test]
    fn test_insert_overwrite() {
        let mut m = IterableMap::new(b"b");
        assert!(m.insert(1, 2).is_none());
        assert_eq!(*m.get(&1).unwrap(), 2);
        assert!(m.insert(1, 3).is_some());
        assert_eq!(*m.get(&1).unwrap(), 3);
    }

    #[test]
    fn test_is_empty() {
        let mut m = IterableMap::new(b"b");
        assert!(m.insert(1, 2).is_none());
        assert!(!m.is_empty());
        assert!(m.remove(&1).is_some());
        assert!(m.is_empty());
    }

    #[test]
    fn test_remove() {
        let mut m = IterableMap::new(b"b");
        m.insert(1, 2);
        assert_eq!(m.remove(&1), Some(2));
        assert_eq!(m.remove(&1), None);
    }

    #[test]
    fn test_remove_entry() {
        let mut m = IterableMap::new(b"b");
        m.insert(1, 2);
        assert_eq!(m.remove_entry(&1), Some((1, 2)));
        assert_eq!(m.remove(&1), None);
    }

    #[test]
    fn test_iterate() {
        let mut m = IterableMap::new(b"b");
        for i in 0..32 {
            assert!(m.insert(i, i * 2).is_none());
        }
        assert_eq!(m.len(), 32);

        let mut observed: u32 = 0;

        for (k, v) in &m {
            assert_eq!(*v, *k * 2);
            observed |= 1 << *k;
        }
        assert_eq!(observed, 0xFFFF_FFFF);
    }

    #[test]
    fn test_find() {
        let mut m = IterableMap::new(b"b");
        assert!(m.get(&1).is_none());
        m.insert(1, 2);
        match m.get(&1) {
            None => panic!(),
            Some(v) => assert_eq!(*v, 2),
        }
    }

    #[test]
    fn test_show() {
        let mut map = IterableMap::new(b"b");
        let empty: IterableMap<i32, i32> = IterableMap::new(b"c");

        map.insert(1, 2);
        map.insert(3, 4);

        let map_str = format!("{:?}", map);

        assert_eq!(map_str, "IterableMap { keys: Vector { len: 2, prefix: [98, 118] }, values: LookupMap { prefix: [98, 109] } }");
        assert_eq!(format!("{:?}", empty), "IterableMap { keys: Vector { len: 0, prefix: [99, 118] }, values: LookupMap { prefix: [99, 109] } }");
    }

    #[test]
    fn test_size_hint() {
        let mut map = IterableMap::new(b"b");

        let xs = [(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6)];

        for v in xs {
            map.insert(v.0, v.1);
        }

        let mut iter = map.iter();

        for _ in iter.by_ref().take(3) {}

        assert_eq!(iter.size_hint(), (3, Some(3)));
    }

    #[test]
    fn test_iter_len() {
        let mut map = IterableMap::new(b"b");

        let xs = [(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6)];

        for v in xs {
            map.insert(v.0, v.1);
        }

        let mut iter = map.iter();

        for _ in iter.by_ref().take(3) {}

        assert_eq!(iter.len(), 3);
    }

    #[test]
    fn test_mut_size_hint() {
        let mut map = IterableMap::new(b"b");

        let xs = [(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6)];

        for v in xs {
            map.insert(v.0, v.1);
        }

        let mut iter = map.iter_mut();

        for _ in iter.by_ref().take(3) {}

        assert_eq!(iter.size_hint(), (3, Some(3)));
    }

    #[test]
    fn test_iter_mut_len() {
        let mut map = IterableMap::new(b"b");

        let xs = [(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6)];

        for v in xs {
            map.insert(v.0, v.1);
        }

        let mut iter = map.iter_mut();

        for _ in iter.by_ref().take(3) {}

        assert_eq!(iter.len(), 3);
    }

    #[test]
    fn test_index() {
        let mut map = IterableMap::new(b"b");

        map.insert(1, 2);
        map.insert(2, 1);
        map.insert(3, 4);

        assert_eq!(map[&2], 1);
    }

    #[test]
    #[should_panic]
    #[allow(clippy::unnecessary_operation)]
    fn test_index_nonexistent() {
        let mut map = IterableMap::new(b"b");

        map.insert(1, 2);
        map.insert(2, 1);
        map.insert(3, 4);

        #[allow(clippy::no_effect)] // false positive lint
        map[&4];
    }

    #[test]
    fn test_entry() {
        let mut map = IterableMap::new(b"b");

        let xs = [(1, 10), (2, 20), (3, 30), (4, 40), (5, 50), (6, 60)];

        for v in xs {
            map.insert(v.0, v.1);
        }

        // Existing key (insert)
        match map.entry(1) {
            Vacant(_) => unreachable!(),
            Occupied(mut view) => {
                assert_eq!(view.get(), &10);
                assert_eq!(view.insert(100), 10);
            }
        }
        assert_eq!(map.get(&1).unwrap(), &100);
        assert_eq!(map.len(), 6);

        // Existing key (update)
        match map.entry(2) {
            Vacant(_) => unreachable!(),
            Occupied(mut view) => {
                let v = view.get_mut();
                let new_v = (*v) * 10;
                *v = new_v;
            }
        }
        assert_eq!(map.get(&2).unwrap(), &200);
        assert_eq!(map.len(), 6);

        // Existing key (take)
        match map.entry(3) {
            Vacant(_) => unreachable!(),
            Occupied(view) => {
                assert_eq!(view.remove(), 30);
            }
        }
        assert_eq!(map.get(&3), None);
        assert_eq!(map.len(), 5);

        // Inexistent key (insert)
        match map.entry(10) {
            Occupied(_) => unreachable!(),
            Vacant(view) => {
                assert_eq!(*view.insert(1000), 1000);
            }
        }
        assert_eq!(map.get(&10).unwrap(), &1000);
        assert_eq!(map.len(), 6);
    }

    #[test]
    fn test_entry_take_doesnt_corrupt() {
        fn check(m: &IterableMap<i32, ()>) {
            for k in m.keys() {
                assert!(m.contains_key(k), "{} is in keys() but not in the map?", k);
            }
        }

        let mut m = IterableMap::new(b"b");

        let mut rng = {
            let seed = u64::from_le_bytes(*b"testseed");
            SmallRng::seed_from_u64(seed)
        };

        // Populate the map with some items.
        for _ in 0..50 {
            let x = rng.gen_range(-10..10);
            m.insert(x, ());
        }

        for _ in 0..1000 {
            let x = rng.gen_range(-10..10);
            match m.entry(x) {
                Vacant(_) => {}
                Occupied(e) => {
                    e.remove();
                }
            }

            check(&m);
        }
    }

    #[test]
    fn test_extend_ref_kv_tuple() {
        let mut a = IterableMap::new(b"b");
        a.insert(0, 0);

        let for_iter: Vec<(i32, i32)> = (0..100).map(|i| (i, i)).collect();
        a.extend(for_iter);

        assert_eq!(a.len(), 100);

        for item in 0..100 {
            assert_eq!(a[&item], item);
        }
    }

    #[test]
    fn test_occupied_entry_key() {
        let mut a = IterableMap::new(b"b");
        let key = "hello there";
        let value = "value goes here";
        assert!(a.is_empty());
        a.insert(key.to_string(), value.to_string());
        assert_eq!(a.len(), 1);
        assert_eq!(a[key], value);

        match a.entry(key.to_string()) {
            Vacant(_) => panic!(),
            Occupied(e) => assert_eq!(key, *e.key()),
        }
        assert_eq!(a.len(), 1);
        assert_eq!(a[key], value);
    }

    #[test]
    fn test_vacant_entry_key() {
        let mut a = IterableMap::new(b"b");
        let key = "hello there";
        let value = "value goes here".to_string();

        assert!(a.is_empty());
        match a.entry(key.to_string()) {
            Occupied(_) => panic!(),
            Vacant(e) => {
                assert_eq!(key, *e.key());
                e.insert(value.clone());
            }
        }
        assert_eq!(a.len(), 1);
        assert_eq!(a[key], value);
    }
}
