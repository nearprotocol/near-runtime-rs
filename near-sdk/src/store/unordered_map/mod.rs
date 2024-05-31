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

pub use entry::{Entry, OccupiedEntry, VacantEntry};

pub use self::iter::{Drain, Iter, IterMut, Keys, Values, ValuesMut};
use super::free_list::FreeListIndex;
use super::{FreeList, LookupMap};

/// A lazily loaded storage map that stores its content directly on the storage trie.
/// This structure is similar to [`near_sdk::store::LookupMap`](crate::store::LookupMap), except
/// that it stores the keys so that [`UnorderedMap`] can be iterable.
///
/// This map stores the values under a hash of the map's `prefix` and [`BorshSerialize`] of the key
/// using the map's [`ToKey`] implementation.
///
/// The default hash function for [`UnorderedMap`] is [`Sha256`] which uses a syscall
/// (or host function) built into the NEAR runtime to hash the key. To use a custom function,
/// use [`with_hasher`]. Alternative builtin hash functions can be found at
/// [`near_sdk::store::key`](crate::store::key).
///
/// # Performance considerations
/// Note that this collection is optimized for fast removes at the expense of key management.
/// If the amount of removes is significantly higher than the amount of inserts the iteration
/// becomes more costly. See [`remove`](UnorderedMap::remove) for details.
/// If this is the use-case - see ['IterableMap`](crate::store::IterableMap).
///
/// # Examples
/// ```
/// use near_sdk::store::UnorderedMap;
///
/// // Initializes a map, the generic types can be inferred to `UnorderedMap<String, u8, Sha256>`
/// // The `b"a"` parameter is a prefix for the storage keys of this data structure.
/// let mut map = UnorderedMap::new(b"a");
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
/// [`UnorderedMap`] also implements an [`Entry API`](Self::entry), which allows
/// for more complex methods of getting, setting, updating and removing keys and
/// their values:
///
/// ```
/// use near_sdk::store::UnorderedMap;
///
/// // type inference lets us omit an explicit type signature (which
/// // would be `UnorderedMap<String, u8>` in this example).
/// let mut player_stats = UnorderedMap::new(b"m");
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
#[deprecated(
    since = "5.0.0",
    note = "Suboptimal iteration performance. See performance considerations doc for details."
)]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct UnorderedMap<K, V, H = Sha256>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    // ser/de is independent of `K` ser/de, `BorshSerialize`/`BorshDeserialize` bounds removed
    #[borsh(bound(serialize = "", deserialize = ""))]
    keys: FreeList<K>,
    // ser/de is independent of `K`, `V`, `H` ser/de, `BorshSerialize`/`BorshDeserialize` bounds removed
    #[borsh(bound(serialize = "", deserialize = ""))]
    values: LookupMap<K, ValueAndIndex<V>, H>,
}

#[near(inside_nearsdk)]
struct ValueAndIndex<V> {
    value: V,
    key_index: FreeListIndex,
}

impl<K, V, H> Drop for UnorderedMap<K, V, H>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    fn drop(&mut self) {
        self.flush()
    }
}

impl<K, V, H> fmt::Debug for UnorderedMap<K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + fmt::Debug,
    V: BorshSerialize,
    H: ToKey,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnorderedMap")
            .field("keys", &self.keys)
            .field("values", &self.values)
            .finish()
    }
}

impl<K, V> UnorderedMap<K, V, Sha256>
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map: UnorderedMap<String, u8> = UnorderedMap::new(b"b");
    /// ```
    #[inline]
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self::with_hasher(prefix)
    }
}

impl<K, V, H> UnorderedMap<K, V, H>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    /// Initialize a [`UnorderedMap`] with a custom hash function.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::{UnorderedMap, key::Keccak256};
    ///
    /// let map = UnorderedMap::<String, String, Keccak256>::with_hasher(b"m");
    /// ```
    pub fn with_hasher<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        let mut vec_key = prefix.into_storage_key();
        let map_key = [vec_key.as_slice(), b"m"].concat();
        vec_key.push(b'v');
        Self { keys: FreeList::new(vec_key), values: LookupMap::with_hasher(map_key) }
    }

    /// Return the amount of elements inside of the map.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map: UnorderedMap<String, u8> = UnorderedMap::new(b"b");
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map: UnorderedMap<String, u8> = UnorderedMap::new(b"b");
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map: UnorderedMap<String, u8> = UnorderedMap::new(b"b");
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
        for k in self.keys.drain() {
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map = UnorderedMap::new(b"m");
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map = UnorderedMap::new(b"m");
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map = UnorderedMap::new(b"m");
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map = UnorderedMap::new(b"m");
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map = UnorderedMap::new(b"m");
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut a = UnorderedMap::new(b"m");
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

impl<K, V, H> UnorderedMap<K, V, H>
where
    K: BorshSerialize + Ord,
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map: UnorderedMap<String, u8> = UnorderedMap::new(b"b");
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map: UnorderedMap<String, u8> = UnorderedMap::new(b"b");
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map: UnorderedMap<String, u8> = UnorderedMap::new(b"b");
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
        let key_index = self.keys.insert(k);
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
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map: UnorderedMap<String, u8> = UnorderedMap::new(b"b");
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
    /// When elements are removed, the underlying vector of keys isn't
    /// rearranged; instead, the removed key is replaced with a placeholder value. These
    /// empty slots are reused on subsequent [`insert`](Self::insert) operations.
    ///
    /// In cases where there are a lot of removals and not a lot of insertions, these leftover
    /// placeholders might make iteration more costly, driving higher gas costs. If you need to
    /// remedy this, take a look at [`defrag`](Self::defrag).
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map: UnorderedMap<String, u8> = UnorderedMap::new(b"b");
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
    /// When elements are removed, the underlying vector of keys isn't
    /// rearranged; instead, the removed key is replaced with a placeholder value. These
    /// empty slots are reused on subsequent [`insert`](Self::insert) operations.
    ///
    /// In cases where there are a lot of removals and not a lot of insertions, these leftover
    /// placeholders might make iteration more costly, driving higher gas costs. If you need to
    /// remedy this, take a look at [`defrag`](Self::defrag).
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map = UnorderedMap::new(b"m");
    /// map.insert(1, "a".to_string());
    /// assert_eq!(map.remove(&1), Some("a".to_string()));
    /// assert_eq!(map.remove(&1), None);
    /// ```
    pub fn remove_entry<Q: ?Sized>(&mut self, k: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q> + BorshDeserialize,
        Q: BorshSerialize + ToOwned<Owned = K>,
    {
        // Remove value
        let old_value = self.values.remove(k)?;

        // Remove key with index if value exists
        let key = self
            .keys
            .remove(old_value.key_index)
            .unwrap_or_else(|| env::panic_err(errors::InconsistentCollectionState::new().into()));

        // Return removed value
        Some((key, old_value.value))
    }

    /// Gets the given key's corresponding entry in the map for in-place manipulation.
    /// ```
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut count = UnorderedMap::new(b"m");
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
    pub fn entry(&mut self, key: K) -> Entry<K, V>
    where
        K: Clone,
    {
        Entry::new(self.values.entry(key), &mut self.keys)
    }
}

impl<K, V, H> UnorderedMap<K, V, H>
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

impl<K, V, H> UnorderedMap<K, V, H>
where
    K: BorshSerialize + BorshDeserialize + Ord + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: ToKey,
{
    /// Remove empty placeholders leftover from calling [`remove`](Self::remove).
    ///
    /// When elements are removed using [`remove`](Self::remove), the underlying vector isn't
    /// rearranged; instead, the removed element is replaced with a placeholder value. These
    /// empty slots are reused on subsequent [`insert`](Self::insert) operations.
    ///
    /// In cases where there are a lot of removals and not a lot of insertions, these leftover
    /// placeholders might make iteration more costly, driving higher gas costs. This method is meant
    /// to remedy that by removing all empty slots from the underlying vector and compacting it.
    ///
    /// Note that this might exceed the available gas amount depending on the amount of free slots,
    /// therefore has to be used with caution.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedMap;
    ///
    /// let mut map = UnorderedMap::new(b"b");
    ///
    /// for i in 0..4 {
    ///     map.insert(i, i);
    /// }
    ///
    /// map.remove(&1);
    /// map.remove(&3);
    ///
    /// map.defrag();
    /// ```
    pub fn defrag(&mut self) {
        self.keys.defrag(|key, new_index| {
            if let Some(existing) = self.values.get_mut(key) {
                existing.key_index = FreeListIndex(new_index);
            }
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::UnorderedMap;
    use crate::test_utils::test_env::setup_free;
    use arbitrary::{Arbitrary, Unstructured};
    use borsh::{to_vec, BorshDeserialize};
    use rand::RngCore;
    use rand::SeedableRng;
    use std::collections::HashMap;

    #[test]
    fn basic_functionality() {
        let mut map = UnorderedMap::new(b"b");
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
        let mut map = UnorderedMap::new(b"b");
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
        let mut map = UnorderedMap::new(b"b");

        map.insert(0u8, 0u8);
        map.insert(1, 1);
        map.insert(2, 2);
        map.insert(3, 3);
        map.remove(&1);
        let iter = map.iter();
        assert_eq!(iter.len(), 3);
        assert_eq!(iter.collect::<Vec<_>>(), [(&0, &0), (&2, &2), (&3, &3)]);

        let iter = map.iter_mut().rev();
        assert_eq!(iter.collect::<Vec<_>>(), [(&3, &mut 3), (&2, &mut 2), (&0, &mut 0)]);

        let mut iter = map.iter();
        assert_eq!(iter.nth(2), Some((&3, &3)));
        // Check fused iterator assumption that each following one will be None
        assert_eq!(iter.next(), None);

        // Double all values
        map.values_mut().for_each(|v| {
            *v *= 2;
        });
        assert_eq!(map.values().collect::<Vec<_>>(), [&0, &4, &6]);

        // Collect all keys
        assert_eq!(map.keys().collect::<Vec<_>>(), [&0, &2, &3]);
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

            let mut um = UnorderedMap::new(b"l");
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
                            um = UnorderedMap::deserialize(&mut serialized.as_slice()).unwrap();
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

    #[test]
    fn defrag() {
        let mut map = UnorderedMap::new(b"b");

        let all_indices = 0..=8;

        for i in all_indices {
            map.insert(i, i);
        }

        let removed = [2, 4, 6];
        let existing = [0, 1, 3, 5, 7, 8];

        for id in removed {
            map.remove(&id);
        }

        map.defrag();

        for i in removed {
            assert_eq!(map.get(&i), None);
        }
        for i in existing {
            assert_eq!(map.get(&i), Some(&i));
        }

        //Check the elements moved during defragmentation
        assert_eq!(map.remove_entry(&7).unwrap(), (7, 7));
        assert_eq!(map.remove_entry(&8).unwrap(), (8, 8));
        assert_eq!(map.remove_entry(&1).unwrap(), (1, 1));
        assert_eq!(map.remove_entry(&3).unwrap(), (3, 3));
    }
}
