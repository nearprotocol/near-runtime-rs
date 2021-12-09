use std::iter::FusedIterator;
use std::ops::Bound;

use borsh::{BorshDeserialize, BorshSerialize};

use super::{expect, LookupMap, Tree, TreeMap};
use crate::crypto_hash::CryptoHasher;

impl<'a, K, V, H> IntoIterator for &'a TreeMap<K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V, H>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K, V, H> IntoIterator for &'a mut TreeMap<K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    type Item = (&'a K, &'a mut V);
    type IntoIter = IterMut<'a, K, V, H>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// An iterator over elements of a [`TreeMap`], in sorted order.
///
/// This `struct` is created by the `iter` method on [`TreeMap`].
pub struct Iter<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize,
    V: BorshSerialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    keys: Keys<'a, K>,
    values: &'a LookupMap<K, V, H>,
}

impl<'a, K, V, H> Iter<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize,
    V: BorshSerialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    pub(super) fn new(map: &'a TreeMap<K, V, H>) -> Self {
        Self { keys: Keys::new_unbounded(&map.tree), values: &map.values }
    }
}

impl<'a, K, V, H> Iterator for Iter<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        <Self as Iterator>::nth(self, 0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let key = self.keys.nth(n)?;
        let entry = expect(self.values.get(key));

        Some((key, &entry))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.keys.size_hint()
    }

    fn count(self) -> usize {
        self.keys.count()
    }
}

impl<'a, K, V, H> ExactSizeIterator for Iter<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
}
impl<'a, K, V, H> FusedIterator for Iter<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
}

impl<'a, K, V, H> DoubleEndedIterator for Iter<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        <Self as DoubleEndedIterator>::nth_back(self, 0)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let key = self.keys.nth_back(n)?;
        let entry = expect(self.values.get(key));

        Some((key, &entry))
    }
}

/// A mutable iterator over elements of a [`TreeMap`], in sorted order.
///
/// This `struct` is created by the `iter_mut` method on [`TreeMap`].
pub struct IterMut<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize,
    V: BorshSerialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    /// Values iterator which contains empty and filled cells.
    keys: Keys<'a, K>,
    /// Exclusive reference to underlying map to lookup values with `keys`.
    values: &'a mut LookupMap<K, V, H>,
}

impl<'a, K, V, H> IterMut<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize,
    V: BorshSerialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    pub(super) fn new(map: &'a mut TreeMap<K, V, H>) -> Self {
        Self { keys: Keys::new_unbounded(&map.tree), values: &mut map.values }
    }
    fn get_entry_mut<'b>(&'b mut self, key: &'a K) -> (&'a K, &'a mut V)
    where
        K: Clone,
        V: BorshDeserialize,
    {
        let entry = expect(self.values.get_mut(key));
        //* SAFETY: The lifetime can be swapped here because we can assert that the iterator
        //*         will only give out one mutable reference for every individual key in the bucket
        //*         during the iteration, and there is no overlap. This operates under the
        //*         assumption that all elements in the bucket are unique and no hash collisions.
        //*         Because we use 32 byte hashes and all keys are verified unique based on the
        //*         `TreeMap` API, this is safe.
        let value = unsafe { &mut *(entry as *mut V) };
        (key, value)
    }
}

impl<'a, K, V, H> Iterator for IterMut<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        <Self as Iterator>::nth(self, 0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let key = self.keys.nth(n)?;
        Some(self.get_entry_mut(key))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.keys.size_hint()
    }

    fn count(self) -> usize {
        self.keys.count()
    }
}

impl<'a, K, V, H> ExactSizeIterator for IterMut<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
}
impl<'a, K, V, H> FusedIterator for IterMut<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
}

impl<'a, K, V, H> DoubleEndedIterator for IterMut<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        <Self as DoubleEndedIterator>::nth_back(self, 0)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let key = self.keys.nth_back(n)?;
        Some(self.get_entry_mut(key))
    }
}

/// An iterator over the keys of a [`TreeMap`], in sorted order.
///
/// This `struct` is created by the `keys` method on [`TreeMap`].
pub struct Keys<'a, K: 'a>
where
    K: BorshSerialize + BorshDeserialize + Ord,
{
    tree: &'a Tree<K>,
    length: u32,
    min: Bound<&'a K>,
    max: Bound<&'a K>,
}

impl<'a, K> Keys<'a, K>
where
    K: BorshSerialize + BorshDeserialize + Ord,
{
    pub(super) fn new(tree: &'a Tree<K>, bounds: (Bound<&'a K>, Bound<&'a K>)) -> Self {
        let (min, max) = bounds;
        Self { tree, length: tree.nodes.len(), min, max }
    }

    pub(super) fn new_unbounded(map: &'a Tree<K>) -> Self {
        Self::new(map, (Bound::Unbounded, Bound::Unbounded))
    }

    fn next_asc(&self) -> Option<&'a K>
    where
        K: Clone,
    {
        match self.min {
            Bound::Unbounded => self.tree.min(),
            Bound::Included(bound) => self.tree.ceil_key(bound),
            Bound::Excluded(bound) => self.tree.higher(bound),
        }
    }

    fn next_desc(&self) -> Option<&'a K>
    where
        K: Clone,
    {
        match self.max {
            Bound::Unbounded => self.tree.max(),
            Bound::Included(bound) => self.tree.floor_key(bound),
            Bound::Excluded(bound) => self.tree.lower(bound),
        }
    }
}

impl<'a, K> Iterator for Keys<'a, K>
where
    // TODO yoink clone bound, probably shouldn't be needed
    K: BorshSerialize + BorshDeserialize + Ord + Clone,
{
    type Item = &'a K;

    fn next(&mut self) -> Option<&'a K> {
        if self.length == 0 {
            // Short circuit if all elements have been iterated.
            return None;
        }

        let next = self.next_asc();
        if let Some(bound) = next {
            // Update minimum bound.
            self.min = Bound::Excluded(bound);

            // Decrease count of potential elements
            self.length -= 1;
        } else {
            // No more elements to iterate, set length to 0 to avoid duplicate lookups.
            // Bounds can never be updated manually once initialized, so this can be done.
            self.length = 0;
        }

        next
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.length as usize;
        (len, Some(len))
    }

    fn count(self) -> usize {
        self.length as usize
    }
}

impl<'a, K> ExactSizeIterator for Keys<'a, K> where
    K: BorshSerialize + BorshDeserialize + Ord + Clone
{
}
impl<'a: 'b, 'b: 'a, K> FusedIterator for Keys<'a, K> where
    K: BorshSerialize + BorshDeserialize + Ord + Clone
{
}

impl<'a: 'b, 'b: 'a, K> DoubleEndedIterator for Keys<'a, K>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
{
    fn next_back(&mut self) -> Option<&'a K> {
        if self.length == 0 {
            // Short circuit if all elements have been iterated.
            return None;
        }

        let next = self.next_desc();
        if let Some(bound) = next {
            // Update maximum bound.
            self.max = Bound::Excluded(bound);

            // Decrease count of potential elements
            self.length -= 1;
        } else {
            // No more elements to iterate, set length to 0 to avoid duplicate lookups.
            // Bounds can never be updated manually once initialized, so this can be done.
            self.length = 0;
        }

        next
    }
}

/// An iterator over the values of a [`TreeMap`], in order by key.
///
/// This `struct` is created by the `values` method on [`TreeMap`].
pub struct Values<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize,
    V: BorshSerialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    inner: Iter<'a, K, V, H>,
}

impl<'a, K, V, H> Values<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize,
    V: BorshSerialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    pub(super) fn new(map: &'a TreeMap<K, V, H>) -> Self {
        Self { inner: map.iter() }
    }
}

impl<'a, K, V, H> Iterator for Values<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        <Self as Iterator>::nth(self, 0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.inner.nth(n).map(|(_, v)| v)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }

    fn count(self) -> usize {
        self.inner.count()
    }
}

impl<'a, K, V, H> ExactSizeIterator for Values<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
}
impl<'a, K, V, H> FusedIterator for Values<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
}

impl<'a, K, V, H> DoubleEndedIterator for Values<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        <Self as DoubleEndedIterator>::nth_back(self, 0)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.inner.nth_back(n).map(|(_, v)| v)
    }
}

/// A mutable iterator over values of a [`TreeMap`], in order by key.
///
/// This `struct` is created by the `values_mut` method on [`TreeMap`].
pub struct ValuesMut<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize,
    V: BorshSerialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    inner: IterMut<'a, K, V, H>,
}

impl<'a, K, V, H> ValuesMut<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize,
    V: BorshSerialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    pub(super) fn new(map: &'a mut TreeMap<K, V, H>) -> Self {
        Self { inner: map.iter_mut() }
    }
}

impl<'a, K, V, H> Iterator for ValuesMut<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    type Item = &'a mut V;

    fn next(&mut self) -> Option<Self::Item> {
        <Self as Iterator>::nth(self, 0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.inner.nth(n).map(|(_, v)| v)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }

    fn count(self) -> usize {
        self.inner.count()
    }
}

impl<'a, K, V, H> ExactSizeIterator for ValuesMut<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
}
impl<'a, K, V, H> FusedIterator for ValuesMut<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
}

impl<'a, K, V, H> DoubleEndedIterator for ValuesMut<'a, K, V, H>
where
    K: BorshSerialize + Ord + BorshDeserialize + Clone,
    V: BorshSerialize + BorshDeserialize,
    H: CryptoHasher<Digest = [u8; 32]>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        <Self as DoubleEndedIterator>::nth_back(self, 0)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.inner.nth_back(n).map(|(_, v)| v)
    }
}
