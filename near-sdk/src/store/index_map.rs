use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};
use near_sdk_macros::near;
use once_cell::unsync::OnceCell;

use crate::utils::StableMap;
use crate::{env, errors, CacheEntry, EntryState, IntoStorageKey};

#[near(inside_nearsdk)]
pub(crate) struct IndexMap<T>
where
    T: BorshSerialize,
{
    pub(crate) prefix: Box<[u8]>,
    /// Cache for loads and intermediate changes to the underlying index map.
    /// The cached entries are wrapped in a [`Box`] to avoid existing pointers from being
    /// invalidated.
    ///
    /// Note: u32 indices are used over usize to have consistent functionality across architectures.
    /// Some functionality would be different from tests to Wasm if exceeding 32-bit length.
    #[borsh(skip, bound(deserialize = ""))] // removes `core::default::Default` bound from T
    pub(crate) cache: StableMap<u32, OnceCell<CacheEntry<T>>>,
}

impl<T> IndexMap<T>
where
    T: BorshSerialize,
{
    /// Create new index map. This creates a mapping of `u32` -> `T` in storage.
    ///
    /// This prefix can be anything that implements [`IntoStorageKey`]. The prefix is used when
    /// storing and looking up values in storage to ensure no collisions with other collections.
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self { prefix: prefix.into_storage_key().into_boxed_slice(), cache: Default::default() }
    }

    fn index_to_lookup_key(prefix: &[u8], index: u32, buf: &mut Vec<u8>) {
        buf.extend_from_slice(prefix);
        buf.extend_from_slice(&index.to_le_bytes());
    }

    /// Flushes the cache and writes all modified values to storage.
    pub fn flush(&mut self) {
        let mut buf = Vec::new();
        // Capacity is prefix length plus bytes needed for u32 bytes (4*u8)
        let mut key_buf = Vec::with_capacity(self.prefix.len() + 4);
        for (k, v) in self.cache.inner().iter_mut() {
            if let Some(v) = v.get_mut() {
                if v.is_modified() {
                    key_buf.clear();
                    Self::index_to_lookup_key(&self.prefix, *k, &mut key_buf);
                    match v.value().as_ref() {
                        Some(modified) => {
                            buf.clear();
                            BorshSerialize::serialize(modified, &mut buf).unwrap_or_else(|_| {
                                env::panic_err(errors::BorshSerializeError::new("element").into())
                            });
                            env::storage_write(&key_buf, &buf);
                        }
                        None => {
                            // Element was removed, clear the storage for the value
                            env::storage_remove(&key_buf);
                        }
                    }

                    // Update state of flushed state as cached, to avoid duplicate writes/removes
                    // while also keeping the cached values in memory.
                    v.replace_state(EntryState::Cached);
                }
            }
        }
    }

    /// Sets a value at a given index to the value provided. If none is provided, this index will
    /// be removed from storage.
    pub fn set(&mut self, index: u32, value: Option<T>) {
        let entry = self.cache.get_mut(index);
        match entry.get_mut() {
            Some(entry) => *entry.value_mut() = value,
            None => {
                let _ = entry.set(CacheEntry::new_modified(value));
            }
        }
    }
}

impl<T> IndexMap<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    fn deserialize_element(raw_element: &[u8]) -> T {
        T::try_from_slice(raw_element).unwrap_or_else(|_| {
            env::panic_err(errors::BorshDeserializeError::new("element").into())
        })
    }

    /// Returns the element by index or `None` if it is not present.
    pub fn get(&self, index: u32) -> Option<&T> {
        let entry = self.cache.get(index).get_or_init(|| {
            let mut buf = Vec::with_capacity(self.prefix.len() + 4);
            Self::index_to_lookup_key(&self.prefix, index, &mut buf);
            let storage_bytes = env::storage_read(&buf);
            let value = storage_bytes.as_deref().map(Self::deserialize_element);
            CacheEntry::new_cached(value)
        });
        entry.value().as_ref()
    }

    /// Returns a mutable reference to the element at the `index` provided.
    pub(crate) fn get_mut_inner(&mut self, index: u32) -> &mut CacheEntry<T> {
        let prefix = &self.prefix;
        let entry = self.cache.get_mut(index);
        entry.get_or_init(|| {
            let mut key = Vec::with_capacity(prefix.len() + 4);
            Self::index_to_lookup_key(prefix, index, &mut key);
            let storage_bytes = env::storage_read(&key);
            let value = storage_bytes.as_deref().map(Self::deserialize_element);
            CacheEntry::new_cached(value)
        });
        let entry = entry.get_mut().unwrap();
        entry
    }

    /// Returns a mutable reference to the element at the `index` provided.
    pub fn get_mut(&mut self, index: u32) -> Option<&mut T> {
        let entry = self.get_mut_inner(index);
        entry.value_mut().as_mut()
    }

    pub fn swap(&mut self, a: u32, b: u32) {
        if a == b {
            // Short circuit if indices are the same, also guarantees uniqueness below
            return;
        }

        let val_a = self.get_mut_inner(a).replace(None);
        let val_b = self.get_mut_inner(b).replace(val_a);
        self.get_mut_inner(a).replace(val_b);
    }

    /// Inserts a element at `index`, returns the evicted element.
    pub fn insert(&mut self, index: u32, element: T) -> Option<T> {
        self.get_mut_inner(index).replace(Some(element))
    }

    /// Removes value at index and returns existing value.
    pub fn remove(&mut self, index: u32) -> Option<T> {
        self.get_mut_inner(index).replace(None)
    }
}

impl<T> fmt::Debug for IndexMap<T>
where
    T: BorshSerialize + BorshDeserialize + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IndexMap").field("prefix", &self.prefix).finish()
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::IndexMap;
    use crate::test_utils::test_env::setup_free;
    use arbitrary::{Arbitrary, Unstructured};
    use rand::RngCore;
    use rand::SeedableRng;
    use std::collections::HashMap;

    #[test]
    fn basic_usage() {
        let mut map = IndexMap::new(b"v".to_vec());

        map.insert(3, 3u8);
        map.insert(43, 43);
        map.swap(3, 43);
        assert_eq!(map.get(3), Some(&43));
        assert_eq!(map.remove(43), Some(3));

        map.swap(1, 3);
        *map.get_mut(1).unwrap() += 2;
        assert_eq!(map.get(1), Some(&45));

        map.set(0, Some(1));

        map.flush();
        assert_eq!(map.get(0), Some(&1));
    }

    #[derive(Arbitrary, Debug)]
    enum Op {
        Insert(u8, u8),
        Remove(u8),
        Flush,
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

            let mut im = IndexMap::new(b"l");
            let mut hm = HashMap::new();
            let u = Unstructured::new(&buf);
            if let Ok(ops) = Vec::<Op>::arbitrary_take_rest(u) {
                for op in ops {
                    match op {
                        Op::Insert(k, v) => {
                            let r1 = im.insert(k as u32, v);
                            let r2 = hm.insert(k, v);
                            assert_eq!(r1, r2)
                        }
                        Op::Remove(k) => {
                            let r1 = im.remove(k as u32);
                            let r2 = hm.remove(&k);
                            assert_eq!(r1, r2)
                        }
                        Op::Flush => {
                            im.flush();
                        }
                        Op::Get(k) => {
                            let r1 = im.get(k as u32);
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
    use crate::store::IndexMap;
    use borsh::{BorshDeserialize, BorshSerialize};
    use std::cell::RefCell;
    use std::vec::Vec;

    thread_local! { static DROP_VECTOR: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) }}

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
            *slot.borrow_mut() = vec![0; 100];
        });

        {
            let mut m = IndexMap::new(b"b");

            DROP_VECTOR.with(|v| {
                for i in 0..100 {
                    assert_eq!(v.borrow()[i], 0);
                }
            });

            for i in 0..100usize {
                let d1 = Droppable::new(i);
                m.insert(i as u32, d1);
            }

            DROP_VECTOR.with(|v| {
                for i in 0..100 {
                    assert_eq!(v.borrow()[i], 1);
                }
            });

            for i in 0..50 {
                let v = m.remove(i as u32);

                assert!(v.is_some());

                DROP_VECTOR.with(|v| {
                    assert_eq!(v.borrow()[i], 1);
                });
            }

            DROP_VECTOR.with(|v| {
                for i in 0..50 {
                    assert_eq!(v.borrow()[i], 0);
                }

                for i in 50..100 {
                    assert_eq!(v.borrow()[i], 1);
                }
            });
        }

        DROP_VECTOR.with(|v| {
            for i in 0..100 {
                assert_eq!(v.borrow()[i], 0);
            }
        });
    }

    #[test]
    fn test_empty_remove() {
        let mut m: IndexMap<bool> = IndexMap::new(b"b");
        assert_eq!(m.remove(0), None);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // FIXME: takes too long
    fn test_lots_of_insertions() {
        let mut m = IndexMap::new(b"b");

        // Try this a few times to make sure we never screw up the IndexMap's
        // internal state.
        for _ in 0..10 {
            for i in 1..1001 {
                assert!(m.insert(i, i).is_none());

                for j in 1..=i {
                    let r = m.get(j);
                    assert_eq!(r, Some(&j));
                }

                for j in i + 1..1001 {
                    let r = m.get(j);
                    assert!(r.is_none());
                }
            }

            for i in 1001..2001 {
                assert!(m.get(i).is_none());
            }

            // remove forwards
            for i in 1..1001 {
                assert!(m.remove(i).is_some());

                for j in 1..=i {
                    assert!(m.get(j).is_none());
                }

                for j in i + 1..1001 {
                    assert!(m.get(j).is_some());
                }
            }

            for i in 1..1001 {
                assert!(m.get(i).is_none());
            }

            for i in 1..1001 {
                assert!(m.insert(i, i).is_none());
            }

            // remove backwards
            for i in (1..1001).rev() {
                assert!(m.remove(i).is_some());

                for j in i..1001 {
                    assert!(m.get(j).is_none());
                }

                for j in 1..i {
                    assert!(m.get(j).is_some());
                }
            }
        }
    }

    #[test]
    fn test_find_mut() {
        let mut m = IndexMap::new(b"b");
        assert!(m.insert(1, 12).is_none());
        assert!(m.insert(2, 8).is_none());
        assert!(m.insert(5, 14).is_none());
        let new = 100;
        match m.get_mut(5) {
            None => panic!(),
            Some(x) => *x = new,
        }
        assert_eq!(m.get(5), Some(&new));
    }

    #[test]
    fn test_insert_overwrite() {
        let mut m = IndexMap::new(b"b");
        assert!(m.insert(1, 2).is_none());
        assert_eq!(*m.get(1).unwrap(), 2);
        assert!(m.insert(1, 3).is_some());
        assert_eq!(*m.get(1).unwrap(), 3);
    }

    #[test]
    fn test_remove() {
        let mut m = IndexMap::new(b"b");
        m.insert(1, 2);
        assert_eq!(m.remove(1), Some(2));
        assert_eq!(m.remove(1), None);
    }

    #[test]
    fn test_find() {
        let mut m = IndexMap::new(b"b");
        assert!(m.get(1).is_none());
        m.insert(1, 2);
        match m.get(1) {
            None => panic!(),
            Some(v) => assert_eq!(*v, 2),
        }
    }

    #[test]
    fn test_show() {
        let mut map = IndexMap::new(b"b");
        let empty: IndexMap<i32> = IndexMap::new(b"c");

        map.insert(1, 2);
        map.insert(3, 4);

        let map_str = format!("{:?}", map);

        assert_eq!(map_str, "IndexMap { prefix: [98] }");
        assert_eq!(format!("{:?}", empty), "IndexMap { prefix: [99] }");
    }
}
