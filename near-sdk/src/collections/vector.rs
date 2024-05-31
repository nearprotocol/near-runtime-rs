//! A vector implemented on a trie. Unlike standard vector does not support insertion and removal
//! of an element results in the last element being placed in the empty position.
use core::ops::Range;
use std::iter::FusedIterator;
use std::marker::PhantomData;

use borsh::{to_vec, BorshDeserialize, BorshSerialize};
use near_sdk_macros::near;

use crate::collections::append_slice;
use crate::{env, errors, IntoStorageKey};

fn expect_consistent_state<T>(val: Option<T>) -> T {
    val.unwrap_or_else(|| env::panic_err(errors::InconsistentCollectionState::new().into()))
}

/// An iterable implementation of vector that stores its content on the trie.
/// Uses the following map: index -> element.
#[near(inside_nearsdk)]
pub struct Vector<T> {
    len: u64,
    prefix: Vec<u8>,
    #[borsh(skip)]
    el: PhantomData<T>,
}

impl<T> Vector<T> {
    /// Returns the number of elements in the vector, also referred to as its size.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Returns `true` if the vector contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Create new vector with zero elements. Use `id` as a unique identifier on the trie.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::collections::Vector;
    /// let mut set: Vector<u32> = Vector::new(b"m");
    /// ```
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self { len: 0, prefix: prefix.into_storage_key(), el: PhantomData }
    }

    /// Helper utility to be able to easily migrate to the new [`Vector`] implementation.
    ///
    /// This new [`Vector`]'s API matches the Rust [`Vec`] API more closely and has a caching
    /// layer to avoid reading/writing redundant times to storage.
    ///
    /// [`Vector`]: crate::store::Vector
    #[cfg(feature = "unstable")]
    pub fn to_v2(&self) -> crate::store::Vector<T>
    where
        T: BorshSerialize,
    {
        crate::store::Vector {
            // Length cannot feasibly exceed u32::MAX, but checked conversion anyway.
            len: self.len.try_into().unwrap(),
            values: crate::store::IndexMap::new(self.prefix.as_slice()),
        }
    }

    fn index_to_lookup_key(&self, index: u64) -> Vec<u8> {
        append_slice(&self.prefix, &index.to_le_bytes()[..])
    }

    /// Returns the serialized element by index or `None` if it is not present.
    pub fn get_raw(&self, index: u64) -> Option<Vec<u8>> {
        if index >= self.len {
            return None;
        }
        let lookup_key = self.index_to_lookup_key(index);
        Some(expect_consistent_state(env::storage_read(&lookup_key)))
    }

    /// Removes an element from the vector and returns it in serialized form.
    /// The removed element is replaced by the last element of the vector.
    /// Does not preserve ordering, but is `O(1)`.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    pub fn swap_remove_raw(&mut self, index: u64) -> Vec<u8> {
        if index >= self.len {
            env::panic_err(errors::IndexOutOfBounds {}.into())
        } else if index + 1 == self.len {
            expect_consistent_state(self.pop_raw())
        } else {
            let lookup_key = self.index_to_lookup_key(index);
            let raw_last_value = self.pop_raw().expect("checked `index < len` above, so `len > 0`");
            if env::storage_write(&lookup_key, &raw_last_value) {
                expect_consistent_state(env::storage_get_evicted())
            } else {
                env::panic_err(errors::InconsistentCollectionState::new().into())
            }
        }
    }

    /// Appends a serialized element to the back of the collection.
    pub fn push_raw(&mut self, raw_element: &[u8]) {
        let lookup_key = self.index_to_lookup_key(self.len);
        self.len += 1;
        env::storage_write(&lookup_key, raw_element);
    }

    /// Removes the last element from a vector and returns it without deserializing, or `None` if it is empty.
    pub fn pop_raw(&mut self) -> Option<Vec<u8>> {
        if self.is_empty() {
            None
        } else {
            let last_index = self.len - 1;
            let last_lookup_key = self.index_to_lookup_key(last_index);

            self.len -= 1;
            let raw_last_value = if env::storage_remove(&last_lookup_key) {
                expect_consistent_state(env::storage_get_evicted())
            } else {
                env::panic_err(errors::InconsistentCollectionState::new().into())
            };
            Some(raw_last_value)
        }
    }

    /// Inserts a serialized element at `index`, returns a serialized evicted element.
    ///
    /// # Panics
    ///
    /// If `index` is out of bounds.
    pub fn replace_raw(&mut self, index: u64, raw_element: &[u8]) -> Vec<u8> {
        if index >= self.len {
            env::panic_err(errors::IndexOutOfBounds {}.into())
        } else {
            let lookup_key = self.index_to_lookup_key(index);
            if env::storage_write(&lookup_key, raw_element) {
                expect_consistent_state(env::storage_get_evicted())
            } else {
                env::panic_err(errors::InconsistentCollectionState::new().into())
            }
        }
    }

    /// Iterate over raw serialized elements.
    pub fn iter_raw(&self) -> RawIter<T> {
        RawIter::new(self)
    }

    /// Extends vector from the given collection of serialized elements.
    pub fn extend_raw<IT: IntoIterator<Item = Vec<u8>>>(&mut self, iter: IT) {
        for el in iter {
            self.push_raw(&el)
        }
    }
}

impl<T> Vector<T> {
    /// Removes all elements from the collection.
    pub fn clear(&mut self) {
        for i in 0..self.len {
            let lookup_key = self.index_to_lookup_key(i);
            env::storage_remove(&lookup_key);
        }
        self.len = 0;
    }
}

impl<T> Vector<T>
where
    T: BorshSerialize,
{
    fn serialize_element(element: &T) -> Vec<u8> {
        to_vec(element)
            .unwrap_or_else(|_| env::panic_err(errors::BorshSerializeError::new("element").into()))
    }

    /// Appends an element to the back of the collection.
    pub fn push(&mut self, element: &T) {
        let raw_element = Self::serialize_element(element);
        self.push_raw(&raw_element);
    }

    /// Extends vector from the given collection.
    pub fn extend<IT: IntoIterator<Item = T>>(&mut self, iter: IT) {
        for el in iter {
            self.push(&el)
        }
    }
}

impl<T> Vector<T>
where
    T: BorshDeserialize,
{
    fn deserialize_element(raw_element: &[u8]) -> T {
        T::try_from_slice(raw_element).unwrap_or_else(|_| {
            env::panic_err(errors::BorshDeserializeError::new("element").into())
        })
    }

    /// Returns the element by index or `None` if it is not present.
    pub fn get(&self, index: u64) -> Option<T> {
        self.get_raw(index).map(|x| Self::deserialize_element(&x))
    }

    /// Removes an element from the vector and returns it.
    /// The removed element is replaced by the last element of the vector.
    /// Does not preserve ordering, but is `O(1)`.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    pub fn swap_remove(&mut self, index: u64) -> T {
        let raw_evicted = self.swap_remove_raw(index);
        Self::deserialize_element(&raw_evicted)
    }

    /// Removes the last element from a vector and returns it, or `None` if it is empty.
    pub fn pop(&mut self) -> Option<T> {
        self.pop_raw().map(|x| Self::deserialize_element(&x))
    }

    /// Iterate over deserialized elements.
    pub fn iter(&self) -> Iter<T> {
        Iter::new(self)
    }

    pub fn to_vec(&self) -> Vec<T> {
        self.iter().collect()
    }
}

impl<T> Vector<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    /// Inserts a element at `index`, returns an evicted element.
    ///
    /// # Panics
    ///
    /// If `index` is out of bounds.
    pub fn replace(&mut self, index: u64, element: &T) -> T {
        let raw_element = Self::serialize_element(element);
        Self::deserialize_element(&self.replace_raw(index, &raw_element))
    }
}

#[cfg(feature = "expensive-debug")]
impl<T: std::fmt::Debug + BorshDeserialize> std::fmt::Debug for Vector<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_vec().fmt(f)
    }
}

#[cfg(not(feature = "expensive-debug"))]
impl<T: std::fmt::Debug + BorshDeserialize> std::fmt::Debug for Vector<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vector").field("len", &self.len).field("prefix", &self.prefix).finish()
    }
}

/// An iterator over raw serialized bytes of each element in the [`Vector`].
pub struct RawIter<'a, T> {
    vec: &'a Vector<T>,
    range: Range<u64>,
}

impl<'a, T> RawIter<'a, T> {
    fn new(vec: &'a Vector<T>) -> Self {
        Self { vec, range: Range { start: 0, end: vec.len() } }
    }

    /// Returns number of elements left to iterate.
    fn remaining(&self) -> usize {
        (self.range.end - self.range.start) as usize
    }
}

impl<'a, T> Iterator for RawIter<'a, T> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        <Self as Iterator>::nth(self, 0)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.remaining();
        (remaining, Some(remaining))
    }

    fn count(self) -> usize {
        self.remaining()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let idx = self.range.nth(n)?;
        self.vec.get_raw(idx)
    }
}

impl<'a, T> ExactSizeIterator for RawIter<'a, T> {}
impl<'a, T> FusedIterator for RawIter<'a, T> {}

impl<'a, T> DoubleEndedIterator for RawIter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        <Self as DoubleEndedIterator>::nth_back(self, 0)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let idx = self.range.nth_back(n)?;
        self.vec.get_raw(idx)
    }
}

/// An iterator over each element deserialized in the [`Vector`].
pub struct Iter<'a, T> {
    inner: RawIter<'a, T>,
}

impl<'a, T> Iter<'a, T> {
    fn new(vec: &'a Vector<T>) -> Self {
        Self { inner: RawIter::new(vec) }
    }
}

impl<'a, T> Iterator for Iter<'a, T>
where
    T: BorshDeserialize,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        <Self as Iterator>::nth(self, 0)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.inner.remaining();
        (remaining, Some(remaining))
    }

    fn count(self) -> usize {
        self.inner.remaining()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.inner.nth(n).map(|raw_element| Vector::deserialize_element(&raw_element))
    }
}

impl<'a, T> ExactSizeIterator for Iter<'a, T> where T: BorshDeserialize {}
impl<'a, T> FusedIterator for Iter<'a, T> where T: BorshDeserialize {}

impl<'a, T> DoubleEndedIterator for Iter<'a, T>
where
    T: BorshDeserialize,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        <Self as DoubleEndedIterator>::nth_back(self, 0)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.inner.nth_back(n).map(|raw_element| Vector::deserialize_element(&raw_element))
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use borsh::BorshDeserialize;
    use rand::{Rng, SeedableRng};

    use crate::collections::Vector;

    #[test]
    fn test_push_pop() {
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(0);
        let mut vec = Vector::new(b"v".to_vec());
        let mut baseline = vec![];
        for _ in 0..500 {
            let value = rng.gen::<u64>();
            vec.push(&value);
            baseline.push(value);
        }
        let actual = vec.to_vec();
        assert_eq!(actual, baseline);
        for _ in 0..1001 {
            assert_eq!(baseline.pop(), vec.pop());
        }
    }

    #[test]
    pub fn test_replace() {
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(1);
        let mut vec = Vector::new(b"v".to_vec());
        let mut baseline = vec![];
        for _ in 0..500 {
            let value = rng.gen::<u64>();
            vec.push(&value);
            baseline.push(value);
        }
        for _ in 0..500 {
            let index = rng.gen::<u64>() % vec.len();
            let value = rng.gen::<u64>();
            let old_value0 = vec.get(index).unwrap();
            let old_value1 = vec.replace(index, &value);
            let old_value2 = baseline[index as usize];
            assert_eq!(old_value0, old_value1);
            assert_eq!(old_value0, old_value2);
            *baseline.get_mut(index as usize).unwrap() = value;
        }
        let actual = vec.to_vec();
        assert_eq!(actual, baseline);
    }

    #[test]
    pub fn test_swap_remove() {
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(2);
        let mut vec = Vector::new(b"v".to_vec());
        let mut baseline = vec![];
        for _ in 0..500 {
            let value = rng.gen::<u64>();
            vec.push(&value);
            baseline.push(value);
        }
        for _ in 0..500 {
            let index = rng.gen::<u64>() % vec.len();
            let old_value0 = vec.get(index).unwrap();
            let old_value1 = vec.swap_remove(index);
            let old_value2 = baseline[index as usize];
            let last_index = baseline.len() - 1;
            baseline.swap(index as usize, last_index);
            baseline.pop();
            assert_eq!(old_value0, old_value1);
            assert_eq!(old_value0, old_value2);
        }
        let actual = vec.to_vec();
        assert_eq!(actual, baseline);
    }

    #[test]
    pub fn test_clear() {
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(3);
        let mut vec = Vector::new(b"v".to_vec());
        for _ in 0..100 {
            for _ in 0..(rng.gen::<u64>() % 20 + 1) {
                let value = rng.gen::<u64>();
                vec.push(&value);
            }
            assert!(!vec.is_empty());
            vec.clear();
            assert!(vec.is_empty());
        }
    }

    #[test]
    pub fn test_extend() {
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(0);
        let mut vec = Vector::new(b"v".to_vec());
        let mut baseline = vec![];
        for _ in 0..100 {
            let value = rng.gen::<u64>();
            vec.push(&value);
            baseline.push(value);
        }

        for _ in 0..100 {
            let mut tmp = vec![];
            for _ in 0..=(rng.gen::<u64>() % 20 + 1) {
                let value = rng.gen::<u64>();
                tmp.push(value);
            }
            baseline.extend(tmp.clone());
            vec.extend(tmp.clone());
        }
        let actual = vec.to_vec();
        assert_eq!(actual, baseline);
    }

    #[test]
    fn test_debug() {
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(4);
        let prefix = b"v".to_vec();
        let mut vec = Vector::new(prefix.clone());
        let mut baseline = vec![];
        for _ in 0..10 {
            let value = rng.gen::<u64>();
            vec.push(&value);
            baseline.push(value);
        }
        let actual = vec.to_vec();
        assert_eq!(actual, baseline);
        for _ in 0..5 {
            assert_eq!(baseline.pop(), vec.pop());
        }
        if cfg!(feature = "expensive-debug") {
            assert_eq!(format!("{:#?}", vec), format!("{:#?}", baseline));
        } else {
            assert_eq!(
                format!("{:?}", vec),
                format!("Vector {{ len: 5, prefix: {:?} }}", vec.prefix)
            );
        }

        #[derive(Debug, BorshDeserialize)]
        #[allow(dead_code)]
        struct WithoutBorshSerialize(u64);

        let deserialize_only_vec =
            Vector::<WithoutBorshSerialize> { len: vec.len(), prefix, el: Default::default() };
        let baseline: Vec<_> = baseline.into_iter().map(WithoutBorshSerialize).collect();
        if cfg!(feature = "expensive-debug") {
            assert_eq!(format!("{:#?}", deserialize_only_vec), format!("{:#?}", baseline));
        } else {
            assert_eq!(
                format!("{:?}", deserialize_only_vec),
                format!("Vector {{ len: 5, prefix: {:?} }}", deserialize_only_vec.prefix)
            );
        }
    }

    #[test]
    pub fn iterator_checks() {
        let mut vec = Vector::new(b"v");
        let mut baseline = vec![];
        for i in 0..10 {
            vec.push(&i);
            baseline.push(i);
        }

        let mut vec_iter = vec.iter();
        let mut bl_iter = baseline.iter();
        assert_eq!(vec_iter.next(), bl_iter.next().copied());
        assert_eq!(vec_iter.next_back(), bl_iter.next_back().copied());
        assert_eq!(vec_iter.nth(3), bl_iter.nth(3).copied());
        assert_eq!(vec_iter.nth_back(2), bl_iter.nth_back(2).copied());

        // Check to make sure indexing overflow is handled correctly
        assert!(vec_iter.nth(5).is_none());
        assert!(bl_iter.nth(5).is_none());

        assert!(vec_iter.next().is_none());
        assert!(bl_iter.next().is_none());

        // Count check
        assert_eq!(vec.iter().count(), baseline.len());
    }
}
