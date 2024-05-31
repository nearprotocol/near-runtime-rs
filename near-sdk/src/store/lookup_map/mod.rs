mod entry;
mod impls;

use std::borrow::Borrow;
use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};
use near_sdk_macros::near;
use once_cell::unsync::OnceCell;

use crate::store::key::{Identity, ToKey};
use crate::utils::{EntryState, StableMap};
use crate::{env, errors, CacheEntry, IntoStorageKey};

pub use entry::{Entry, OccupiedEntry, VacantEntry};

/// A non-iterable, lazily loaded storage map that stores its content directly on the storage trie.
///
/// This map stores the values under a hash of the map's `prefix` and [`BorshSerialize`] of the key
/// and transformed using the map's [`ToKey`] implementation.
///
/// The default hash function for [`LookupMap`] is [`Identity`] which just prefixes the serialized
/// key object and uses these bytes as the key. This is to be backwards-compatible with
/// [`collections::LookupMap`](crate::collections::LookupMap) and be fast for small keys.
/// To use a custom function, use [`with_hasher`]. Alternative builtin hash functions can be found
/// at [`near_sdk::store::key`](crate::store::key).
///
/// # Examples
/// ```
/// use near_sdk::store::LookupMap;
///
/// // Initializes a map, the generic types can be inferred to `LookupMap<String, u8, Identity>`
/// // The `b"a"` parameter is a prefix for the storage keys of this data structure.
/// let mut map = LookupMap::new(b"a");
///
/// map.set("test".to_string(), Some(7u8));
/// assert!(map.contains_key("test"));
/// assert_eq!(map.get("test"), Some(&7u8));
///
/// let prev = map.insert("test".to_string(), 5u8);
/// assert_eq!(prev, Some(7u8));
/// assert_eq!(map["test"], 5u8);
/// ```
///
/// [`LookupMap`] also implements an [`Entry API`](Self::entry), which allows
/// for more complex methods of getting, setting, updating and removing keys and
/// their values:
///
/// ```
/// use near_sdk::store::LookupMap;
///
/// // type inference lets us omit an explicit type signature (which
/// // would be `LookupMap<String, u8>` in this example).
/// let mut player_stats = LookupMap::new(b"m");
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

#[near(inside_nearsdk)]
pub struct LookupMap<K, V, H = Identity>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    prefix: Box<[u8]>,
    /// Cache for loads and intermediate changes to the underlying vector.
    /// The cached entries are wrapped in a [`Box`] to avoid existing pointers from being
    /// invalidated.
    #[borsh(skip, bound(deserialize = ""))] // removes `core::default::Default` from `K`/`V`
    cache: StableMap<K, EntryAndHash<V, H::KeyType>>,
}

struct EntryAndHash<V, T> {
    value: OnceCell<CacheEntry<V>>,
    hash: OnceCell<T>,
}

impl<V, T> Default for EntryAndHash<V, T> {
    fn default() -> Self {
        Self { value: Default::default(), hash: Default::default() }
    }
}

impl<K, V, H> Drop for LookupMap<K, V, H>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    fn drop(&mut self) {
        self.flush()
    }
}

impl<K, V, H> fmt::Debug for LookupMap<K, V, H>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LookupMap").field("prefix", &self.prefix).finish()
    }
}

impl<K, V> LookupMap<K, V, Identity>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
{
    /// Create a new [`LookupMap`] with the prefix provided.
    ///
    /// This prefix can be anything that implements [`IntoStorageKey`]. The prefix is used when
    /// storing and looking up values in storage to ensure no collisions with other collections.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::LookupMap;
    ///
    /// let mut map: LookupMap<u32, String> = LookupMap::new(b"m");
    /// ```
    #[inline]
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self::with_hasher(prefix)
    }
}

impl<K, V, H> LookupMap<K, V, H>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    /// Initialize a [`LookupMap`] with a custom hash function.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::{LookupMap, key::Keccak256};
    ///
    /// let map = LookupMap::<String, String, Keccak256>::with_hasher(b"m");
    /// ```
    pub fn with_hasher<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self { prefix: prefix.into_storage_key().into_boxed_slice(), cache: Default::default() }
    }

    /// Overwrites the current value for the given key.
    ///
    /// This function will not load the existing value from storage and return the value in storage.
    /// Use [`LookupMap::insert`] if you need the previous value.
    ///
    /// Calling `set` with a `None` value will delete the entry from storage.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::LookupMap;
    ///
    /// let mut map = LookupMap::new(b"m");
    ///
    /// map.set("test".to_string(), Some(7u8));
    /// assert!(map.contains_key("test"));
    ///
    /// //Delete the entry from storage
    /// map.set("test".to_string(), None);
    /// assert!(!map.contains_key("test"));
    /// ```
    pub fn set(&mut self, key: K, value: Option<V>) {
        let entry = self.cache.get_mut(key);
        match entry.value.get_mut() {
            Some(entry) => *entry.value_mut() = value,
            None => {
                let _ = entry.value.set(CacheEntry::new_modified(value));
            }
        }
    }
}

impl<K, V, H> LookupMap<K, V, H>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize + BorshDeserialize,
    H: ToKey,
{
    fn deserialize_element(bytes: &[u8]) -> V {
        V::try_from_slice(bytes).unwrap_or_else(|_| {
            env::panic_err(errors::BorshDeserializeError::new("element").into())
        })
    }

    fn load_element<Q: ?Sized>(prefix: &[u8], key: &Q) -> (H::KeyType, Option<V>)
    where
        Q: BorshSerialize,
        K: Borrow<Q>,
    {
        let key = H::to_key(prefix, key, &mut Vec::new());
        let storage_bytes = env::storage_read(key.as_ref());
        (key, storage_bytes.as_deref().map(Self::deserialize_element))
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`BorshSerialize`] and [`ToOwned<Owned = K>`](ToOwned) on the borrowed form *must* match those for
    /// the key type.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::LookupMap;
    ///
    /// let mut map: LookupMap<u32, String> = LookupMap::new(b"m");
    ///
    /// map.insert(1, "a".to_string());
    /// assert_eq!(map.get(&1), Some(&"a".to_string()));
    /// assert_eq!(map.get(&2), None);
    /// ```
    pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: BorshSerialize + ToOwned<Owned = K>,
    {
        //* ToOwned bound, which forces a clone, is required to be able to keep the key in the cache
        let cached = self.cache.get(k.to_owned());
        let entry = cached.value.get_or_init(|| {
            let (key, element) = Self::load_element(&self.prefix, k);
            let _ = cached.hash.set(key);
            CacheEntry::new_cached(element)
        });
        entry.value().as_ref()
    }

    pub(crate) fn get_mut_inner<Q: ?Sized>(&mut self, k: &Q) -> &mut CacheEntry<V>
    where
        K: Borrow<Q>,
        Q: BorshSerialize + ToOwned<Owned = K>,
    {
        let prefix = &self.prefix;
        //* ToOwned bound, which forces a clone, is required to be able to keep the key in the cache
        let entry = self.cache.get_mut(k.to_owned());
        entry.value.get_or_init(|| {
            let (key, value) = Self::load_element(prefix, k);
            let _ = entry.hash.set(key);
            CacheEntry::new_cached(value)
        });
        let entry = entry.value.get_mut().unwrap_or_else(|| env::abort());
        entry
    }

    /// Returns a mutable reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`BorshSerialize`] and [`ToOwned<Owned = K>`](ToOwned) on the borrowed form *must* match those for
    /// the key type.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::LookupMap;
    ///
    /// let mut map: LookupMap<u32, String> = LookupMap::new(b"m");
    /// map.insert(1, "a".to_string());
    /// if let Some(x) = map.get_mut(&1) {
    ///     *x = "b".to_string();
    ///     assert_eq!(map[&1], "b".to_string());
    /// }
    /// ```
    pub fn get_mut<Q: ?Sized>(&mut self, k: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: BorshSerialize + ToOwned<Owned = K>,
    {
        self.get_mut_inner(k).value_mut().as_mut()
    }

    /// Inserts a key-value pair into the map.
    ///
    /// If the map did not have this key present, [`None`] is returned.
    ///
    /// If the map did have this key present, the value is updated, and the old
    /// value is returned. The key is not updated, though; this matters for
    /// types that can be `==` without being identical.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::LookupMap;
    ///
    /// let mut map: LookupMap<u32, String> = LookupMap::new(b"m");
    /// assert_eq!(map.insert(37, "a".to_string()), None);
    /// assert_eq!(map.contains_key(&37), true);
    ///
    /// map.insert(37, "b".to_string());
    /// assert_eq!(map.insert(37, "c".to_string()), Some("b".to_string()));
    /// assert_eq!(map[&37], "c".to_string());
    /// ```
    pub fn insert(&mut self, k: K, v: V) -> Option<V>
    where
        K: Clone,
    {
        self.get_mut_inner(&k).replace(Some(v))
    }

    /// Returns `true` if the map contains a value for the specified key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`BorshSerialize`] and [`ToOwned<Owned = K>`](ToOwned) on the borrowed form *must* match those for
    /// the key type.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::LookupMap;
    ///
    /// let mut map: LookupMap<u32, String> = LookupMap::new(b"m");
    /// map.insert(1, "a".to_string());
    /// assert_eq!(map.contains_key(&1), true);
    /// assert_eq!(map.contains_key(&2), false);
    /// ```
    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: BorshSerialize + ToOwned<Owned = K> + Ord,
    {
        // Check cache before checking storage
        let contains = self
            .cache
            .map_value_ref(k, |v| v.value.get().and_then(|s| s.value().as_ref()).is_some());
        if let Some(is_some) = contains {
            return is_some;
        }

        // Value is not in cache, check if storage has value for given key.
        let storage_key = H::to_key(&self.prefix, k, &mut Vec::new());
        let contains = env::storage_has_key(storage_key.as_ref());

        if !contains {
            // If value not in cache and not in storage, can set a cached `None`
            let cache = self.cache.get(k.to_owned());
            let _ = cache.value.set(CacheEntry::new_cached(None));
            let _ = cache.hash.set(storage_key);
        }
        contains
    }

    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`BorshSerialize`] and [`ToOwned<Owned = K>`](ToOwned) on the borrowed form *must* match those for
    /// the key type.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::LookupMap;
    ///
    /// let mut map: LookupMap<u32, String> = LookupMap::new(b"m");
    /// map.insert(1, "a".to_string());
    /// assert_eq!(map.remove(&1), Some("a".to_string()));
    /// assert_eq!(map.remove(&1), None);
    /// ```
    pub fn remove<Q: ?Sized>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: BorshSerialize + ToOwned<Owned = K>,
    {
        self.get_mut_inner(k).replace(None)
    }

    /// Gets the given key's corresponding entry in the map for in-place manipulation.
    /// ```
    /// use near_sdk::store::LookupMap;
    ///
    /// let mut count = LookupMap::new(b"m");
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
        let entry = self.get_mut_inner(&key);
        if entry.value().is_some() {
            // Value exists in cache and is `Some`
            Entry::Occupied(OccupiedEntry { key, entry })
        } else {
            // Value exists in cache, but is `None`
            Entry::Vacant(VacantEntry { key, entry })
        }
    }
}

impl<K, V, H> LookupMap<K, V, H>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    /// Flushes the intermediate values of the map before this is called when the structure is
    /// [`Drop`]ed. This will write all modified values to storage but keep all cached values
    /// in memory.
    pub fn flush(&mut self) {
        let mut buf = Vec::new();
        for (k, v) in self.cache.inner().iter_mut() {
            if let Some(val) = v.value.get_mut() {
                if val.is_modified() {
                    let prefix = &self.prefix;
                    let key = v.hash.get_or_init(|| {
                        buf.clear();
                        H::to_key(prefix, k, &mut buf)
                    });
                    match val.value().as_ref() {
                        Some(modified) => {
                            buf.clear();
                            BorshSerialize::serialize(modified, &mut buf).unwrap_or_else(|_| {
                                env::panic_err(errors::BorshSerializeError::new("element").into())
                            });
                            env::storage_write(key.as_ref(), &buf);
                        }
                        None => {
                            // Element was removed, clear the storage for the value
                            env::storage_remove(key.as_ref());
                        }
                    }

                    // Update state of flushed state as cached, to avoid duplicate writes/removes
                    // while also keeping the cached values in memory.
                    val.replace_state(EntryState::Cached);
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::LookupMap;
    use crate::env;
    use crate::store::key::{Keccak256, ToKey};
    use crate::test_utils::test_env::setup_free;
    use arbitrary::{Arbitrary, Unstructured};
    use rand::seq::SliceRandom;
    use rand::RngCore;
    use rand::{Rng, SeedableRng};
    use std::collections::HashMap;

    #[test]
    fn test_insert() {
        let mut map = LookupMap::new(b"m");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(0);
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            let value = rng.gen::<u64>();
            map.insert(key, value);
            assert_eq!(*map.get(&key).unwrap(), value);
        }
    }

    #[test]
    fn test_insert_has_key() {
        let mut map = LookupMap::new(b"m");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(0);
        let mut key_to_value = HashMap::new();
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            let value = rng.gen::<u64>();
            map.insert(key, value);
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
    fn test_insert_remove() {
        let mut map = LookupMap::new(b"m");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(1);
        let mut keys = vec![];
        let mut key_to_value = HashMap::new();
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            let value = rng.gen::<u64>();
            keys.push(key);
            key_to_value.insert(key, value);
            map.insert(key, value);
        }
        keys.shuffle(&mut rng);
        for key in keys {
            let actual = map.remove(&key).unwrap();
            assert_eq!(actual, key_to_value[&key]);
        }
    }

    #[test]
    fn test_remove_last_reinsert() {
        let mut map = LookupMap::new(b"m");
        let key1 = 1u64;
        let value1 = 2u64;
        map.insert(key1, value1);
        let key2 = 3u64;
        let value2 = 4u64;
        map.insert(key2, value2);

        let actual_value2 = map.remove(&key2).unwrap();
        assert_eq!(actual_value2, value2);

        let actual_insert_value2 = map.insert(key2, value2);
        assert_eq!(actual_insert_value2, None);
    }

    #[test]
    fn test_insert_override_remove() {
        let mut map = LookupMap::new(b"m");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(2);
        let mut keys = vec![];
        let mut key_to_value = HashMap::new();
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            let value = rng.gen::<u64>();
            keys.push(key);
            key_to_value.insert(key, value);
            map.insert(key, value);
        }
        keys.shuffle(&mut rng);
        for key in &keys {
            let value = rng.gen::<u64>();
            let actual = map.insert(*key, value).unwrap();
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
    fn test_get_non_existent() {
        let mut map = LookupMap::new(b"m");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(3);
        let mut key_to_value = HashMap::new();
        for _ in 0..500 {
            let key = rng.gen::<u64>() % 20_000;
            let value = rng.gen::<u64>();
            key_to_value.insert(key, value);
            map.insert(key, value);
        }
        for _ in 0..500 {
            let key = rng.gen::<u64>() % 20_000;
            assert_eq!(map.get(&key), key_to_value.get(&key));
        }
    }

    #[test]
    fn size_of_map() {
        assert_eq!(core::mem::size_of::<LookupMap<u8, u8>>(), 48);
    }

    #[test]
    fn identity_compat_v1() {
        use crate::collections::LookupMap as LM1;

        let mut lm1 = LM1::new(b"m");
        lm1.insert(&8u8, &"Some value".to_string());
        lm1.insert(&0, &"Other".to_string());
        assert_eq!(lm1.get(&8), Some("Some value".to_string()));

        let mut lm2 = LookupMap::new(b"m");
        assert_eq!(lm2.get(&8u8), Some(&"Some value".to_string()));
        assert_eq!(lm2.remove(&0), Some("Other".to_string()));
        *lm2.get_mut(&8).unwrap() = "New".to_string();
        lm2.flush();

        assert!(!lm1.contains_key(&0));
        assert_eq!(lm1.get(&8), Some("New".to_string()));
    }

    #[test]
    fn test_extend() {
        let mut map = LookupMap::new(b"m");
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(4);
        let mut key_to_value = HashMap::new();
        for _ in 0..100 {
            let key = rng.gen::<u64>();
            let value = rng.gen::<u64>();
            key_to_value.insert(key, value);
            map.insert(key, value);
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
            assert_eq!(*map.get(&key).unwrap(), value);
        }
    }

    #[test]
    fn flush_on_drop() {
        let mut map = LookupMap::<_, _, Keccak256>::with_hasher(b"m");

        // Set a value, which does not write to storage yet
        map.set(5u8, Some(8u8));

        // Create duplicate which references same data
        assert_eq!(map[&5], 8);

        let storage_key = Keccak256::to_key(b"m", &5, &mut Vec::new());
        assert!(!env::storage_has_key(&storage_key));

        drop(map);

        let dup_map = LookupMap::<u8, u8, Keccak256>::with_hasher(b"m");

        // New map can now load the value
        assert_eq!(dup_map[&5], 8);
    }

    #[derive(Arbitrary, Debug)]
    enum Op {
        Insert(u8, u8),
        Set(u8, Option<u8>),
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

            let mut lm = LookupMap::new(b"l");
            let mut hm = HashMap::new();
            let u = Unstructured::new(&buf);
            if let Ok(ops) = Vec::<Op>::arbitrary_take_rest(u) {
                for op in ops {
                    match op {
                        Op::Insert(k, v) => {
                            let r1 = lm.insert(k, v);
                            let r2 = hm.insert(k, v);
                            assert_eq!(r1, r2)
                        }
                        Op::Set(k, v) => {
                            lm.set(k, v);

                            if let Some(val) = v {
                                hm.insert(k, val);
                            } else {
                                hm.remove(&k);
                            }

                            // Extra get just to make sure set happened correctly
                            assert_eq!(lm.get(&k), hm.get(&k));
                        }
                        Op::Remove(k) => {
                            let r1 = lm.remove(&k);
                            let r2 = hm.remove(&k);
                            assert_eq!(r1, r2)
                        }
                        Op::Flush => {
                            lm.flush();
                        }
                        Op::Restore => {
                            lm = LookupMap::new(b"l");
                        }
                        Op::Get(k) => {
                            let r1 = lm.get(&k);
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
    use crate::store::LookupMap;
    use borsh::{BorshDeserialize, BorshSerialize};
    use std::cell::RefCell;
    use std::usize;
    use std::vec::Vec;

    #[test]
    fn test_insert() {
        let mut m = LookupMap::new(b"b");
        assert!(m.insert(1, 2).is_none());
        assert!(m.insert(2, 4).is_none());
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
            let mut m = LookupMap::new(b"b");

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
                    assert_eq!(v.borrow()[i], 1);
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
                    assert_eq!(v.borrow()[i], 1);
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
    fn test_empty_remove() {
        let mut m: LookupMap<i32, bool> = LookupMap::new(b"b");
        assert_eq!(m.remove(&0), None);
    }

    #[test]
    fn test_empty_entry() {
        let mut m: LookupMap<i32, bool> = LookupMap::new(b"b");
        match m.entry(0) {
            Occupied(_) => panic!(),
            Vacant(_) => {}
        }
        assert!(*m.entry(0).or_insert(true));
    }

    #[test]
    #[cfg_attr(miri, ignore)] // FIXME: takes too long
    fn test_lots_of_insertions() {
        let mut m = LookupMap::new(b"b");

        // Try this a few times to make sure we never screw up the LookupMap's
        // internal state.
        for _ in 0..10 {
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
        let mut m = LookupMap::new(b"b");
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
        let mut m = LookupMap::new(b"b");
        assert!(m.insert(1, 2).is_none());
        assert_eq!(*m.get(&1).unwrap(), 2);
        assert!(m.insert(1, 3).is_some());
        assert_eq!(*m.get(&1).unwrap(), 3);
    }

    #[test]
    fn test_remove() {
        let mut m = LookupMap::new(b"b");
        m.insert(1, 2);
        assert_eq!(m.remove(&1), Some(2));
        assert_eq!(m.remove(&1), None);
    }

    #[test]
    fn test_find() {
        let mut m = LookupMap::new(b"b");
        assert!(m.get(&1).is_none());
        m.insert(1, 2);
        match m.get(&1) {
            None => panic!(),
            Some(v) => assert_eq!(*v, 2),
        }
    }

    #[test]
    fn test_show() {
        let mut map = LookupMap::new(b"b");
        let empty: LookupMap<i32, i32> = LookupMap::new(b"c");

        map.insert(1, 2);
        map.insert(3, 4);

        let map_str = format!("{:?}", map);

        assert_eq!(map_str, "LookupMap { prefix: [98] }");
        assert_eq!(format!("{:?}", empty), "LookupMap { prefix: [99] }");
    }

    #[test]
    fn test_index() {
        let mut map = LookupMap::new(b"b");

        map.insert(1, 2);
        map.insert(2, 1);
        map.insert(3, 4);

        assert_eq!(map[&2], 1);
    }

    #[test]
    #[should_panic]
    #[allow(clippy::unnecessary_operation)]
    fn test_index_nonexistent() {
        let mut map = LookupMap::new(b"b");

        map.insert(1, 2);
        map.insert(2, 1);
        map.insert(3, 4);

        #[allow(clippy::no_effect)] // false positive lint
        map[&4];
    }

    #[test]
    fn test_entry() {
        let mut map = LookupMap::new(b"b");

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

        // Existing key (take)
        match map.entry(3) {
            Vacant(_) => unreachable!(),
            Occupied(view) => {
                assert_eq!(view.remove(), 30);
            }
        }
        assert_eq!(map.get(&3), None);

        // Inexistent key (insert)
        match map.entry(10) {
            Occupied(_) => unreachable!(),
            Vacant(view) => {
                assert_eq!(*view.insert(1000), 1000);
            }
        }
        assert_eq!(map.get(&10).unwrap(), &1000);
    }

    #[test]
    fn test_extend_ref_kv_tuple() {
        let mut a = LookupMap::new(b"b");
        a.insert(0, 0);

        let for_iter: Vec<(i32, i32)> = (0..100).map(|i| (i, i)).collect();
        a.extend(for_iter);

        for item in 0..100 {
            assert_eq!(a[&item], item);
        }
    }

    #[test]
    fn test_occupied_entry_key() {
        let mut a = LookupMap::new(b"b");
        let key = "hello there";
        let value = "value goes here";
        a.insert(key.to_string(), value.to_string());
        assert_eq!(a[key], value);

        match a.entry(key.to_string()) {
            Vacant(_) => panic!(),
            Occupied(e) => assert_eq!(key, *e.key()),
        }
        assert_eq!(a[key], value);
    }

    #[test]
    fn test_vacant_entry_key() {
        let mut a = LookupMap::new(b"b");
        let key = "hello there";
        let value = "value goes here".to_string();

        match a.entry(key.to_string()) {
            Occupied(_) => panic!(),
            Vacant(e) => {
                assert_eq!(key, *e.key());
                e.insert(value.clone());
            }
        }
        assert_eq!(a[key], value);
    }
}
