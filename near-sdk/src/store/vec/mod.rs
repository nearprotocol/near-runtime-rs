//! A growable array type with values persisted to storage and lazily loaded.
//!
//! Values in the [`Vector`] are kept in an in-memory cache and are only persisted on [`Drop`].
//!
//! Vectors ensure they never allocate more than [`u32::MAX`] bytes. [`u32`] is used rather than
//! [`usize`] as in [`Vec`] to ensure consistent behavior on different targets.
//!
//! # Examples
//!
//! You can explicitly create a [`Vector`] with [`Vector::new`]:
//!
//! ```
//! use near_sdk::store::Vector;
//!
//! let v: Vector<i32> = Vector::new(b"a");
//! ```
//!
//! You can [`push`](Vector::push) values onto the end of a vector (which will grow the vector
//! as needed):
//!
//! ```
//! use near_sdk::store::Vector;
//!
//! let mut v: Vector<i32> = Vector::new(b"a");
//!
//! v.push(3);
//! ```
//!
//! Popping values works in much the same way:
//!
//! ```
//! use near_sdk::store::Vector;
//!
//! let mut v: Vector<i32> = Vector::new(b"a");
//! v.extend([1, 2]);
//!
//! let two = v.pop();
//! ```
//!
//! Vectors also support indexing (through the [`Index`] and [`IndexMut`] traits):
//!
//! ```
//! use near_sdk::store::Vector;
//!
//! let mut v: Vector<i32> = Vector::new(b"a");
//! v.extend([1, 2, 3]);
//!
//! let three = v[2];
//! v[1] = v[1] + 5;
//! ```
//!
//! [`Index`]: std::ops::Index
//! [`IndexMut`]: std::ops::IndexMut

mod impls;
mod iter;

use std::{
    fmt,
    ops::{Bound, Range, RangeBounds},
};

use borsh::{BorshDeserialize, BorshSerialize};
use near_sdk_macros::NearSchema;

pub use self::iter::{Drain, Iter, IterMut};
use crate::{env, errors, IntoStorageKey};

use super::IndexMap;

fn expect_consistent_state<T>(val: Option<T>) -> T {
    val.unwrap_or_else(|| env::panic_err(errors::InconsistentCollectionState::new().into()))
}

/// An iterable implementation of vector that stores its content on the trie. This implementation
/// will load and store values in the underlying storage lazily.
///
/// Uses the following map: index -> element. Because the data is sharded to avoid reading/writing
/// large chunks of data, the values cannot be accessed as a contiguous piece of memory.
///
/// This implementation will cache all changes and loads and only updates values that are changed
/// in storage after it's dropped through it's [`Drop`] implementation. These changes can be updated
/// in storage before the variable is dropped by using [`Vector::flush`]. During the lifetime of
/// this type, storage will only be read a maximum of one time per index and only written once per
/// index unless specifically flushed.
///
/// This type should be a drop in replacement for [`Vec`] in most cases and will provide contracts
/// a vector structure which scales much better as the contract data grows.
///
/// # Examples
/// ```
/// use near_sdk::store::Vector;
///
/// let mut vec = Vector::new(b"a");
/// assert!(vec.is_empty());
///
/// vec.push(1);
/// vec.push(2);
///
/// assert_eq!(vec.len(), 2);
/// assert_eq!(vec[0], 1);
///
/// assert_eq!(vec.pop(), Some(2));
/// assert_eq!(vec.len(), 1);
///
/// vec[0] = 7;
/// assert_eq!(vec[0], 7);
///
/// vec.extend([1, 2, 3].iter().copied());
/// assert!(Iterator::eq(vec.into_iter(), [7, 1, 2, 3].iter()));
/// ```
#[derive(NearSchema, BorshSerialize, BorshDeserialize)]
#[inside_nearsdk]
#[abi(borsh)]
pub struct Vector<T>
where
    T: BorshSerialize,
{
    pub(crate) len: u32,
    // ser/de is independent of `T` ser/de, `BorshSerialize`/`BorshDeserialize`/`BorshSchema` bounds removed
    #[cfg_attr(not(feature = "abi"), borsh(bound(serialize = "", deserialize = "")))]
    #[cfg_attr(
        feature = "abi",
        borsh(bound(serialize = "", deserialize = ""), schema(params = ""))
    )]
    pub(crate) values: IndexMap<T>,
}

#[test]
fn collections_vec_not_backwards_compatible() {
    use crate::collections::Vector as Vec1;

    let mut v1 = Vec1::new(b"m");
    v1.extend([1u8, 2, 3, 4]);
    // Old collections serializes length as `u64` when new serializes as `u32`.
    assert!(Vector::<u8>::try_from_slice(&borsh::to_vec(&v1).unwrap()).is_err());
}

impl<T> Vector<T>
where
    T: BorshSerialize,
{
    /// Returns the number of elements in the vector, also referred to as its size.
    /// This function returns a `u32` rather than the [`Vec`] equivalent of `usize` to have
    /// consistency between targets.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec = Vector::new(b"a");
    /// vec.push(1);
    /// vec.push(2);
    /// assert_eq!(vec.len(), 2);
    /// ```
    pub fn len(&self) -> u32 {
        self.len
    }

    /// Returns `true` if the vector contains no elements.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec = Vector::new(b"a");
    /// assert!(vec.is_empty());
    ///
    /// vec.push(1);
    /// assert!(!vec.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Create new vector with zero elements. Prefixes storage access with the prefix provided.
    ///
    /// This prefix can be anything that implements [`IntoStorageKey`]. The prefix is used when
    /// storing and looking up values in storage to ensure no collisions with other collections.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec: Vector<u8> = Vector::new(b"a");
    /// ```
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self { len: 0, values: IndexMap::new(prefix) }
    }

    /// Removes all elements from the collection. This will remove all storage values for the
    /// length of the [`Vector`].
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec = Vector::new(b"a");
    /// vec.push(1);
    ///
    /// vec.clear();
    ///
    /// assert!(vec.is_empty());
    /// ```
    pub fn clear(&mut self) {
        for i in 0..self.len {
            self.values.set(i, None);
        }
        self.len = 0;
    }

    /// Flushes the cache and writes all modified values to storage.
    ///
    /// This operation is performed on [`Drop`], but this method can be called to persist
    /// intermediate writes in cases where [`Drop`] is not called or to identify storage changes.
    pub fn flush(&mut self) {
        self.values.flush();
    }

    /// Sets a value at a given index to the value provided. This does not shift values after the
    /// index to the right.
    ///
    /// The reason to use this over modifying with [`Vector::get_mut`] or
    /// [`IndexMut::index_mut`](core::ops::IndexMut::index_mut) is to avoid loading the existing
    /// value from storage. This method will just write the new value.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec = Vector::new(b"v");
    /// vec.push("test".to_string());
    ///
    /// vec.set(0,"new_value".to_string());
    ///
    /// assert_eq!(vec.get(0),Some(&"new_value".to_string()));
    /// ```
    pub fn set(&mut self, index: u32, value: T) {
        if index >= self.len() {
            env::panic_err(errors::IndexOutOfBounds {}.into());
        }

        self.values.set(index, Some(value));
    }

    /// Appends an element to the back of the collection.
    ///
    /// # Panics
    ///
    /// Panics if new length exceeds `u32::MAX`
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec = Vector::new(b"v");
    /// vec.push("test".to_string());
    ///
    /// assert!(!vec.is_empty());
    /// ```
    pub fn push(&mut self, element: T) {
        let last_idx = self.len();
        self.len = self
            .len
            .checked_add(1)
            .unwrap_or_else(|| env::panic_err(errors::IndexOutOfBounds {}.into()));
        self.set(last_idx, element)
    }
}

impl<T> Vector<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    /// Returns the element by index or `None` if it is not present.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec = Vector::new(b"v");
    /// vec.push("test".to_string());
    ///
    /// assert_eq!(Some(&"test".to_string()), vec.get(0));
    /// assert_eq!(None, vec.get(3));
    /// ```
    pub fn get(&self, index: u32) -> Option<&T> {
        if index >= self.len() {
            return None;
        }
        self.values.get(index)
    }

    /// Returns a mutable reference to the element at the `index` provided.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec = Vector::new(b"v");
    /// let x = vec![0, 1, 2];
    /// vec.extend(x);
    ///
    /// if let Some(elem) = vec.get_mut(1) {
    ///     *elem = 42;
    /// }
    ///
    /// let actual: Vec<_> = vec.iter().cloned().collect();
    /// assert_eq!(actual, &[0, 42, 2]);
    /// ```
    pub fn get_mut(&mut self, index: u32) -> Option<&mut T> {
        if index >= self.len {
            return None;
        }
        self.values.get_mut(index)
    }

    pub(crate) fn swap(&mut self, a: u32, b: u32) {
        if a >= self.len() || b >= self.len() {
            env::panic_err(errors::IndexOutOfBounds {}.into());
        }

        self.values.swap(a, b);
    }

    /// Removes an element from the vector and returns it.
    /// The removed element is replaced by the last element of the vector.
    /// Does not preserve ordering, but is `O(1)`.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec: Vector<u8> = Vector::new(b"v");
    /// vec.extend([1, 2, 3, 4]);
    ///
    /// assert_eq!(vec.swap_remove(1), 2);
    /// assert_eq!(vec.iter().copied().collect::<Vec<_>>(), &[1, 4, 3]);
    ///
    /// assert_eq!(vec.swap_remove(0), 1);
    /// assert_eq!(vec.iter().copied().collect::<Vec<_>>(), &[3, 4]);
    /// ```
    pub fn swap_remove(&mut self, index: u32) -> T {
        if self.is_empty() {
            env::panic_err(errors::IndexOutOfBounds {}.into());
        }

        self.swap(index, self.len() - 1);
        expect_consistent_state(self.pop())
    }

    /// Removes the last element from a vector and returns it, or [`None`] if it is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec = Vector::new(b"v");
    /// vec.extend([1, 2, 3]);
    ///
    /// assert_eq!(vec.pop(), Some(3));
    /// assert_eq!(vec.pop(), Some(2));
    /// ```
    pub fn pop(&mut self) -> Option<T> {
        let new_idx = self.len.checked_sub(1)?;
        let prev = self.values.remove(new_idx);
        self.len = new_idx;
        prev
    }

    /// Inserts a element at `index`, returns an evicted element.
    ///
    /// # Panics
    ///
    /// If `index` is out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec = Vector::new(b"v");
    /// vec.push("test".to_string());
    ///
    /// vec.replace(0,"replaced".to_string());
    ///
    /// assert_eq!(vec.get(0), Some(&"replaced".to_string()));
    /// ```
    pub fn replace(&mut self, index: u32, element: T) -> T {
        if index >= self.len {
            env::panic_err(errors::IndexOutOfBounds {}.into());
        }
        self.values.insert(index, element).unwrap()
    }

    /// Returns an iterator over the vector. This iterator will lazily load any values iterated
    /// over from storage.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec = Vector::new(b"v");
    /// vec.extend([1, 2, 4]);
    /// let mut iterator = vec.iter();
    ///
    /// assert_eq!(iterator.next(), Some(&1));
    /// assert_eq!(iterator.next(), Some(&2));
    /// assert_eq!(iterator.next(), Some(&4));
    /// assert_eq!(iterator.next(), None);
    /// ```
    pub fn iter(&self) -> Iter<T> {
        Iter::new(self)
    }

    /// Returns an iterator over the [`Vector`] that allows modifying each value. This iterator
    /// will lazily load any values iterated over from storage.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec = Vector::new(b"v");
    /// vec.extend([1u32, 2, 4]);
    ///
    /// for elem in vec.iter_mut() {
    ///     *elem += 2;
    /// }
    /// assert_eq!(vec.iter().copied().collect::<Vec<_>>(), &[3u32, 4, 6]);
    /// ```
    pub fn iter_mut(&mut self) -> IterMut<T> {
        IterMut::new(self)
    }

    /// Creates a draining iterator that removes the specified range in the vector
    /// and yields the removed items.
    ///
    /// When the iterator **is** dropped, all elements in the range are removed
    /// from the vector, even if the iterator was not fully consumed. If the
    /// iterator **is not** dropped (with [`mem::forget`](std::mem::forget) for example),
    /// the collection will be left in an inconsistent state.
    ///
    /// This will not panic on invalid ranges (`end > length` or `end < start`) and instead the
    /// iterator will just be empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::Vector;
    ///
    /// let mut vec: Vector<u32> = Vector::new(b"v");
    /// vec.extend(vec![1, 2, 3]);
    ///
    /// let u: Vec<_> = vec.drain(1..).collect();
    /// assert_eq!(vec.iter().copied().collect::<Vec<_>>(), &[1]);
    /// assert_eq!(u, &[2, 3]);
    ///
    /// // A full range clears the vector, like `clear()` does
    /// vec.drain(..);
    /// assert!(vec.is_empty());
    /// ```
    pub fn drain<R>(&mut self, range: R) -> Drain<T>
    where
        R: RangeBounds<u32>,
    {
        let start = match range.start_bound() {
            Bound::Excluded(i) => i
                .checked_add(1)
                .unwrap_or_else(|| env::panic_err(errors::IndexOutOfBounds {}.into())),
            Bound::Included(i) => *i,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Excluded(i) => *i,
            Bound::Included(i) => i
                .checked_add(1)
                .unwrap_or_else(|| env::panic_err(errors::IndexOutOfBounds {}.into())),
            Bound::Unbounded => self.len(),
        };

        // Note: don't need to do bounds check if end < start, will just return None when iterating
        // This will also cap the max length at the length of the vector.
        Drain::new(self, Range { start, end: core::cmp::min(end, self.len()) })
    }
}

impl<T> fmt::Debug for Vector<T>
where
    T: BorshSerialize + BorshDeserialize + fmt::Debug,
{
    #[cfg(feature = "expensive-debug")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.iter().collect::<Vec<_>>(), f)
    }

    #[cfg(not(feature = "expensive-debug"))]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vector")
            .field("len", &self.len)
            .field("prefix", &self.values.prefix)
            .finish()
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use arbitrary::{Arbitrary, Unstructured};
    use borsh::{to_vec, BorshDeserialize};
    use rand::{Rng, RngCore, SeedableRng};
    use std::ops::{Bound, IndexMut};

    use super::Vector;
    use crate::{store::IndexMap, test_utils::test_env::setup_free};

    #[test]
    fn test_push_pop() {
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(0);
        let mut vec = Vector::new(b"v".to_vec());
        let mut baseline = vec![];
        for _ in 0..500 {
            let value = rng.gen::<u64>();
            vec.push(value);
            baseline.push(value);
        }
        let actual: Vec<u64> = vec.iter().cloned().collect();
        assert_eq!(actual, baseline);
        for _ in 0..501 {
            assert_eq!(baseline.pop(), vec.pop());
        }
    }

    #[test]
    #[should_panic]
    fn test_set_panic() {
        let mut vec = Vector::new(b"b");
        vec.set(2, 0);
    }

    #[test]
    fn test_get_mut_none() {
        let mut vec: Vector<bool> = Vector::new(b"b");
        assert!(vec.get_mut(2).is_none());
    }

    #[test]
    #[should_panic]
    fn test_drain_panic() {
        let mut vec: Vector<bool> = Vector::new(b"b");
        vec.drain(..=u32::MAX);
    }

    #[test]
    #[should_panic]
    fn test_drain_panic_2() {
        let mut vec: Vector<bool> = Vector::new(b"b");
        vec.drain((Bound::Excluded(u32::MAX), Bound::Included(u32::MAX)));
    }

    #[test]
    fn test_replace_method() {
        let mut vec = Vector::new(b"b");
        vec.push(10);
        vec.replace(0, 2);
        assert_eq!(vec[0], 2);
    }

    #[test]
    #[should_panic]
    fn test_replace_method_panic() {
        let mut vec = Vector::new(b"b");
        vec.replace(0, 2);
    }

    #[test]
    pub fn test_replace() {
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(1);
        let mut vec = Vector::new(b"v".to_vec());
        let mut baseline = vec![];
        for _ in 0..500 {
            let value = rng.gen::<u64>();
            vec.push(value);
            baseline.push(value);
        }
        for _ in 0..500 {
            let index = rng.gen::<u32>() % vec.len();
            let value = rng.gen::<u64>();
            let old_value0 = vec[index];
            let old_value1 = core::mem::replace(vec.get_mut(index).unwrap(), value);
            let old_value2 = baseline[index as usize];
            assert_eq!(old_value0, old_value1);
            assert_eq!(old_value0, old_value2);
            *baseline.get_mut(index as usize).unwrap() = value;
        }
        let actual: Vec<_> = vec.iter().cloned().collect();
        assert_eq!(actual, baseline);
    }

    #[test]
    pub fn test_swap_remove() {
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(2);
        let mut vec = Vector::new(b"v".to_vec());
        let mut baseline = vec![];
        for _ in 0..500 {
            let value = rng.gen::<u64>();
            vec.push(value);
            baseline.push(value);
        }
        for _ in 0..500 {
            let index = rng.gen::<u32>() % vec.len();
            let old_value0 = vec[index];
            let old_value1 = vec.swap_remove(index);
            let old_value2 = baseline[index as usize];
            let last_index = baseline.len() - 1;
            baseline.swap(index as usize, last_index);
            baseline.pop();
            assert_eq!(old_value0, old_value1);
            assert_eq!(old_value0, old_value2);
        }
        let actual: Vec<_> = vec.iter().cloned().collect();
        assert_eq!(actual, baseline);
    }

    #[test]
    #[should_panic]
    pub fn test_swap_remove_panic() {
        let mut vec: Vector<bool> = Vector::new(b"v".to_vec());
        vec.swap_remove(1);
    }

    #[test]
    #[should_panic]
    pub fn test_swap_panic() {
        let mut vec: Vector<bool> = Vector::new(b"v".to_vec());
        vec.swap(1, 2);
    }

    #[test]
    pub fn test_clear() {
        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(3);
        let mut vec = Vector::new(b"v".to_vec());
        for _ in 0..100 {
            for _ in 0..(rng.gen::<u64>() % 20 + 1) {
                let value = rng.gen::<u64>();
                vec.push(value);
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
            vec.push(value);
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
        let actual: Vec<_> = vec.iter().cloned().collect();
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
            vec.push(value);
            baseline.push(value);
        }
        let actual: Vec<_> = vec.iter().cloned().collect();
        assert_eq!(actual, baseline);
        for _ in 0..5 {
            assert_eq!(baseline.pop(), vec.pop());
        }
        if cfg!(feature = "expensive-debug") {
            assert_eq!(format!("{:#?}", vec), format!("{:#?}", baseline));
        } else {
            assert_eq!(
                format!("{:?}", vec),
                format!("Vector {{ len: 5, prefix: {:?} }}", vec.values.prefix)
            );
        }

        // * The storage is reused in the second part of this test, need to flush
        vec.flush();

        use near_sdk_macros::near;

        #[near(inside_nearsdk)]
        #[derive(Debug)]
        struct TestType(u64);

        let deserialize_only_vec =
            Vector::<TestType> { len: vec.len(), values: IndexMap::new(prefix) };
        let baseline: Vec<_> = baseline.into_iter().map(TestType).collect();
        if cfg!(feature = "expensive-debug") {
            assert_eq!(format!("{:#?}", deserialize_only_vec), format!("{:#?}", baseline));
        } else {
            assert_eq!(
                format!("{:?}", deserialize_only_vec),
                format!("Vector {{ len: 5, prefix: {:?} }}", deserialize_only_vec.values.prefix)
            );
        }
    }

    #[test]
    pub fn iterator_checks() {
        let mut vec = Vector::new(b"v");
        let mut baseline = vec![];
        for i in 0..10 {
            vec.push(i);
            baseline.push(i);
        }

        let mut vec_iter = vec.iter();
        let mut bl_iter = baseline.iter();
        assert_eq!(vec_iter.next(), bl_iter.next());
        assert_eq!(vec_iter.next_back(), bl_iter.next_back());
        assert_eq!(vec_iter.nth(3), bl_iter.nth(3));
        assert_eq!(vec_iter.nth_back(2), bl_iter.nth_back(2));

        // Check to make sure indexing overflow is handled correctly
        assert!(vec_iter.nth(5).is_none());
        assert!(bl_iter.nth(5).is_none());

        assert!(vec_iter.next().is_none());
        assert!(bl_iter.next().is_none());

        // Count check
        assert_eq!(vec.iter().count(), baseline.len());
    }

    #[test]
    pub fn iterator_mut_checks() {
        let mut vec = Vector::new(b"v");
        let mut baseline = vec![];
        for i in 0..10 {
            vec.push(i);
            baseline.push(i);
        }

        let mut vec_iter = vec.iter_mut();
        let mut bl_iter = baseline.iter_mut();
        assert_eq!(vec_iter.next(), bl_iter.next());
        assert_eq!(vec_iter.next_back(), bl_iter.next_back());
        assert_eq!(vec_iter.nth(3), bl_iter.nth(3));
        assert_eq!(vec_iter.nth_back(2), bl_iter.nth_back(2));

        // Check to make sure indexing overflow is handled correctly
        assert!(vec_iter.nth(5).is_none());
        assert!(bl_iter.nth(5).is_none());

        assert!(vec_iter.next().is_none());
        assert!(bl_iter.next().is_none());

        // Count check
        assert_eq!(vec.iter().count(), baseline.len());
    }

    #[test]
    fn drain_iterator() {
        let mut vec = Vector::new(b"v");
        let mut baseline = vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        vec.extend(baseline.clone());

        assert!(Iterator::eq(vec.drain(1..=3), baseline.drain(1..=3)));
        assert_eq!(vec.iter().copied().collect::<Vec<_>>(), vec![0, 4, 5, 6, 7, 8, 9]);

        // Test incomplete drain
        {
            let mut drain = vec.drain(0..3);
            let mut b_drain = baseline.drain(0..3);
            assert_eq!(drain.next(), b_drain.next());
            assert_eq!(drain.next(), b_drain.next());
            assert_eq!(drain.count(), 1);
        }

        // 7 elements, drained 3
        assert_eq!(vec.len(), 4);

        // Test incomplete drain over limit
        {
            let mut drain = vec.drain(2..);
            let mut b_drain = baseline.drain(2..);
            assert_eq!(drain.next(), b_drain.next());
        }

        // Drain rest
        assert!(Iterator::eq(vec.drain(..), baseline.drain(..)));

        // Test double ended iterator functions
        let mut vec = Vector::new(b"v");
        let mut baseline = vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        vec.extend(baseline.clone());

        {
            let mut drain = vec.drain(1..8);
            let mut b_drain = baseline.drain(1..8);
            assert_eq!(drain.nth(1), b_drain.nth(1));
            assert_eq!(drain.nth_back(2), b_drain.nth_back(2));
            assert_eq!(drain.len(), b_drain.len());
        }

        assert_eq!(vec.len() as usize, baseline.len());
        assert!(Iterator::eq(vec.iter(), baseline.iter()));

        assert!(Iterator::eq(vec.drain(..), baseline.drain(..)));
        crate::mock::with_mocked_blockchain(|m| assert!(m.take_storage().is_empty()));
    }

    #[test]
    fn test_indexing() {
        let mut v: Vector<i32> = Vector::new(b"b");
        v.push(10);
        v.push(20);
        assert_eq!(v[0], 10);
        assert_eq!(v[1], 20);
        let mut x: u32 = 0;
        assert_eq!(v[x], 10);
        assert_eq!(v[x + 1], 20);
        x += 1;
        assert_eq!(v[x], 20);
        assert_eq!(v[x - 1], 10);
    }

    #[test]
    #[should_panic]
    fn test_index_panic() {
        let v: Vector<bool> = Vector::new(b"b");
        let _ = v[1];
    }

    #[test]
    fn test_index_mut() {
        let mut v: Vector<i32> = Vector::new(b"b");
        v.push(10);
        v.push(20);
        *v.index_mut(0) += 1;
        assert_eq!(v[0], 11);
        assert_eq!(v[1], 20);
        let mut x: u32 = 0;
        assert_eq!(v[x], 11);
        assert_eq!(v[x + 1], 20);
        x += 1;
        assert_eq!(v[x], 20);
        assert_eq!(v[x - 1], 11);
    }

    #[derive(Arbitrary, Debug)]
    enum Op {
        Push(u8),
        Pop,
        Set(u32, u8),
        Remove(u32),
        Flush,
        Reset,
        Get(u32),
        Swap(u32, u32),
    }

    #[test]
    #[should_panic]
    fn test_index_mut_panic() {
        let mut v: Vector<bool> = Vector::new(b"b");
        v.index_mut(1);
    }

    #[test]
    fn arbitrary() {
        setup_free();

        let mut rng = rand_xorshift::XorShiftRng::seed_from_u64(0);
        let mut buf = vec![0; 4096];
        for _ in 0..1024 {
            // Clear storage in-between runs
            crate::mock::with_mocked_blockchain(|b| b.take_storage());
            rng.fill_bytes(&mut buf);

            let mut sv = Vector::new(b"v");
            let mut mv = Vec::new();
            let u = Unstructured::new(&buf);
            if let Ok(ops) = Vec::<Op>::arbitrary_take_rest(u) {
                for op in ops {
                    match op {
                        Op::Push(v) => {
                            sv.push(v);
                            mv.push(v);
                            assert_eq!(sv.len() as usize, mv.len());
                        }
                        Op::Pop => {
                            assert_eq!(sv.pop(), mv.pop());
                            assert_eq!(sv.len() as usize, mv.len());
                        }
                        Op::Set(k, v) => {
                            if sv.is_empty() {
                                continue;
                            }
                            let k = k % sv.len();

                            sv.set(k, v);
                            mv[k as usize] = v;

                            // Extra get just to make sure set happened correctly
                            assert_eq!(sv[k], mv[k as usize]);
                        }
                        Op::Remove(i) => {
                            if sv.is_empty() {
                                continue;
                            }
                            let i = i % sv.len();
                            let r1 = sv.swap_remove(i);
                            let r2 = mv.swap_remove(i as usize);
                            assert_eq!(r1, r2);
                            assert_eq!(sv.len() as usize, mv.len());
                        }
                        Op::Flush => {
                            sv.flush();
                        }
                        Op::Reset => {
                            let serialized = to_vec(&sv).unwrap();
                            sv = Vector::deserialize(&mut serialized.as_slice()).unwrap();
                        }
                        Op::Get(k) => {
                            let r1 = sv.get(k);
                            let r2 = mv.get(k as usize);
                            assert_eq!(r1, r2)
                        }
                        Op::Swap(i1, i2) => {
                            if sv.is_empty() {
                                continue;
                            }
                            let i1 = i1 % sv.len();
                            let i2 = i2 % sv.len();
                            sv.swap(i1, i2);
                            mv.swap(i1 as usize, i2 as usize)
                        }
                    }
                }
            }

            // After all operations, compare both vectors
            assert!(Iterator::eq(sv.iter(), mv.iter()));
        }
    }

    #[test]
    fn serialized_bytes() {
        use borsh::{BorshDeserialize, BorshSerialize};

        let mut vec = Vector::new(b"v".to_vec());
        vec.push("Some data");
        let serialized = to_vec(&vec).unwrap();

        // Expected to serialize len then prefix
        let mut expected_buf = Vec::new();
        1u32.serialize(&mut expected_buf).unwrap();
        (b"v".to_vec()).serialize(&mut expected_buf).unwrap();

        assert_eq!(serialized, expected_buf);
        drop(vec);
        let vec = Vector::<String>::deserialize(&mut serialized.as_slice()).unwrap();
        assert_eq!(vec[0], "Some data");
    }
}
