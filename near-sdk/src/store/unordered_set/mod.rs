// This suppresses the depreciation warnings for uses of UnorderedSet in this module
#![allow(deprecated)]

mod impls;
mod iter;

pub use self::iter::{Difference, Drain, Intersection, Iter, SymmetricDifference, Union};
use super::{FreeList, LookupMap};
use crate::store::free_list::FreeListIndex;
use crate::store::key::{Sha256, ToKey};
use crate::{env, errors, IntoStorageKey};
use borsh::{BorshDeserialize, BorshSerialize};
use std::borrow::Borrow;
use std::fmt;

/// A lazily loaded storage set that stores its content directly on the storage trie.
/// This structure is similar to [`near_sdk::store::LookupSet`](crate::store::LookupSet), except
/// that it keeps track of the elements so that [`UnorderedSet`] can be iterable among other things.
///
/// As with the [`LookupSet`] type, an `UnorderedSet` requires that the elements
/// implement the [`BorshSerialize`] and [`Ord`] traits. This can frequently be achieved by
/// using `#[derive(BorshSerialize, Ord)]`. Some functions also require elements to implement the
/// [`BorshDeserialize`] trait.
///
/// This set stores the values under a hash of the set's `prefix` and [`BorshSerialize`] of the
/// element using the set's [`ToKey`] implementation.
///
/// The default hash function for [`UnorderedSet`] is [`Sha256`] which uses a syscall
/// (or host function) built into the NEAR runtime to hash the element. To use a custom function,
/// use [`with_hasher`]. Alternative builtin hash functions can be found at
/// [`near_sdk::store::key`](crate::store::key).
///
/// # Performance considerations
/// Note that this collection is optimized for fast removes at the expense of key management.
/// If the amount of removes is significantly higher than the amount of inserts the iteration
/// becomes more costly. See [`remove`](UnorderedSet::remove) for details.
/// If this is the use-case - see ['IterableSet`](crate::store::IterableSet).
///
/// # Examples
///
/// ```
/// use near_sdk::store::UnorderedSet;
///
/// // Initializes a set, the generic types can be inferred to `UnorderedSet<String, Sha256>`
/// // The `b"a"` parameter is a prefix for the storage keys of this data structure.
/// let mut set = UnorderedSet::new(b"a");
///
/// set.insert("test".to_string());
/// assert!(set.contains("test"));
/// assert!(set.remove("test"));
/// ```
///
/// [`UnorderedSet`] also implements various binary operations, which allow
/// for iterating various combinations of two sets.
///
/// ```
/// use near_sdk::store::UnorderedSet;
/// use std::collections::HashSet;
///
/// let mut set1 = UnorderedSet::new(b"m");
/// set1.insert(1);
/// set1.insert(2);
/// set1.insert(3);
///
/// let mut set2 = UnorderedSet::new(b"n");
/// set2.insert(2);
/// set2.insert(3);
/// set2.insert(4);
///
/// assert_eq!(
///     set1.union(&set2).collect::<HashSet<_>>(),
///     [1, 2, 3, 4].iter().collect()
/// );
/// assert_eq!(
///     set1.intersection(&set2).collect::<HashSet<_>>(),
///     [2, 3].iter().collect()
/// );
/// assert_eq!(
///     set1.difference(&set2).collect::<HashSet<_>>(),
///     [1].iter().collect()
/// );
/// assert_eq!(
///     set1.symmetric_difference(&set2).collect::<HashSet<_>>(),
///     [1, 4].iter().collect()
/// );
/// ```
///
/// [`with_hasher`]: Self::with_hasher
/// [`LookupSet`]: crate::store::LookupSet
#[derive(BorshDeserialize, BorshSerialize)]
#[deprecated(
    since = "5.0.0",
    note = "Suboptimal iteration performance. See performance considerations doc for details."
)]
pub struct UnorderedSet<T, H = Sha256>
where
    T: BorshSerialize + Ord,
    H: ToKey,
{
    // ser/de is independent of `T` ser/de, `BorshSerialize`/`BorshDeserialize` bounds removed
    #[borsh(bound(serialize = "", deserialize = ""))]
    elements: FreeList<T>,
    // ser/de is independent of `T`, `H` ser/de, `BorshSerialize`/`BorshDeserialize` bounds removed
    #[borsh(bound(serialize = "", deserialize = ""))]
    index: LookupMap<T, FreeListIndex, H>,
}

impl<T, H> Drop for UnorderedSet<T, H>
where
    T: BorshSerialize + Ord,
    H: ToKey,
{
    fn drop(&mut self) {
        self.flush()
    }
}

impl<T, H> fmt::Debug for UnorderedSet<T, H>
where
    T: BorshSerialize + Ord + BorshDeserialize + fmt::Debug,
    H: ToKey,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnorderedSet")
            .field("elements", &self.elements)
            .field("index", &self.index)
            .finish()
    }
}

impl<T> UnorderedSet<T, Sha256>
where
    T: BorshSerialize + Ord,
{
    /// Create a new iterable set. Use `prefix` as a unique prefix for keys.
    ///
    /// This prefix can be anything that implements [`IntoStorageKey`]. The prefix is used when
    /// storing and looking up values in storage to ensure no collisions with other collections.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedSet;
    ///
    /// let mut map: UnorderedSet<String> = UnorderedSet::new(b"b");
    /// ```
    #[inline]
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self::with_hasher(prefix)
    }
}

impl<T, H> UnorderedSet<T, H>
where
    T: BorshSerialize + Ord,
    H: ToKey,
{
    /// Initialize a [`UnorderedSet`] with a custom hash function.
    ///
    /// # Example
    /// ```
    /// use near_sdk::store::key::Keccak256;
    /// use near_sdk::store::UnorderedSet;
    ///
    /// let map = UnorderedSet::<String, Keccak256>::with_hasher(b"m");
    /// ```
    pub fn with_hasher<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        let mut vec_key = prefix.into_storage_key();
        let map_key = [vec_key.as_slice(), b"m"].concat();
        vec_key.push(b'v');
        Self { elements: FreeList::new(vec_key), index: LookupMap::with_hasher(map_key) }
    }

    /// Returns the number of elements in the set.
    pub fn len(&self) -> u32 {
        self.elements.len()
    }

    /// Returns true if the set contains no elements.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Clears the set, removing all values.
    pub fn clear(&mut self)
    where
        T: BorshDeserialize + Clone,
    {
        for e in self.elements.drain() {
            self.index.set(e, None);
        }
    }

    /// Visits the values representing the difference, i.e., the values that are in `self` but not
    /// in `other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedSet;
    ///
    /// let mut set1 = UnorderedSet::new(b"m");
    /// set1.insert("a".to_string());
    /// set1.insert("b".to_string());
    /// set1.insert("c".to_string());
    ///
    /// let mut set2 = UnorderedSet::new(b"n");
    /// set2.insert("b".to_string());
    /// set2.insert("c".to_string());
    /// set2.insert("d".to_string());
    ///
    /// // Can be seen as `set1 - set2`.
    /// for x in set1.difference(&set2) {
    ///     println!("{}", x); // Prints "a"
    /// }
    /// ```
    pub fn difference<'a>(&'a self, other: &'a UnorderedSet<T, H>) -> Difference<'a, T, H>
    where
        T: BorshDeserialize,
    {
        Difference::new(self, other)
    }

    /// Visits the values representing the symmetric difference, i.e., the values that are in
    /// `self` or in `other` but not in both.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedSet;
    ///
    /// let mut set1 = UnorderedSet::new(b"m");
    /// set1.insert("a".to_string());
    /// set1.insert("b".to_string());
    /// set1.insert("c".to_string());
    ///
    /// let mut set2 = UnorderedSet::new(b"n");
    /// set2.insert("b".to_string());
    /// set2.insert("c".to_string());
    /// set2.insert("d".to_string());
    ///
    /// // Prints "a", "d" in arbitrary order.
    /// for x in set1.symmetric_difference(&set2) {
    ///     println!("{}", x);
    /// }
    /// ```
    pub fn symmetric_difference<'a>(
        &'a self,
        other: &'a UnorderedSet<T, H>,
    ) -> SymmetricDifference<'a, T, H>
    where
        T: BorshDeserialize + Clone,
    {
        SymmetricDifference::new(self, other)
    }

    /// Visits the values representing the intersection, i.e., the values that are both in `self`
    /// and `other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedSet;
    ///
    /// let mut set1 = UnorderedSet::new(b"m");
    /// set1.insert("a".to_string());
    /// set1.insert("b".to_string());
    /// set1.insert("c".to_string());
    ///
    /// let mut set2 = UnorderedSet::new(b"n");
    /// set2.insert("b".to_string());
    /// set2.insert("c".to_string());
    /// set2.insert("d".to_string());
    ///
    /// // Prints "b", "c" in arbitrary order.
    /// for x in set1.intersection(&set2) {
    ///     println!("{}", x);
    /// }
    /// ```
    pub fn intersection<'a>(&'a self, other: &'a UnorderedSet<T, H>) -> Intersection<'a, T, H>
    where
        T: BorshDeserialize,
    {
        Intersection::new(self, other)
    }

    /// Visits the values representing the union, i.e., all the values in `self` or `other`, without
    /// duplicates.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedSet;
    ///
    /// let mut set1 = UnorderedSet::new(b"m");
    /// set1.insert("a".to_string());
    /// set1.insert("b".to_string());
    /// set1.insert("c".to_string());
    ///
    /// let mut set2 = UnorderedSet::new(b"n");
    /// set2.insert("b".to_string());
    /// set2.insert("c".to_string());
    /// set2.insert("d".to_string());
    ///
    /// // Prints "a", "b", "c", "d" in arbitrary order.
    /// for x in set1.union(&set2) {
    ///     println!("{}", x);
    /// }
    /// ```
    pub fn union<'a>(&'a self, other: &'a UnorderedSet<T, H>) -> Union<'a, T, H>
    where
        T: BorshDeserialize + Clone,
    {
        Union::new(self, other)
    }

    /// Returns `true` if `self` has no elements in common with `other`. This is equivalent to
    /// checking for an empty intersection.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedSet;
    ///
    /// let mut set1 = UnorderedSet::new(b"m");
    /// set1.insert("a".to_string());
    /// set1.insert("b".to_string());
    /// set1.insert("c".to_string());
    ///
    /// let mut set2 = UnorderedSet::new(b"n");
    ///
    /// assert_eq!(set1.is_disjoint(&set2), true);
    /// set2.insert("d".to_string());
    /// assert_eq!(set1.is_disjoint(&set2), true);
    /// set2.insert("a".to_string());
    /// assert_eq!(set1.is_disjoint(&set2), false);
    /// ```
    pub fn is_disjoint(&self, other: &UnorderedSet<T, H>) -> bool
    where
        T: BorshDeserialize + Clone,
    {
        if self.len() <= other.len() {
            self.iter().all(|v| !other.contains(v))
        } else {
            other.iter().all(|v| !self.contains(v))
        }
    }

    /// Returns `true` if the set is a subset of another, i.e., `other` contains at least all the
    /// values in `self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedSet;
    ///
    /// let mut sup = UnorderedSet::new(b"m");
    /// sup.insert("a".to_string());
    /// sup.insert("b".to_string());
    /// sup.insert("c".to_string());
    ///
    /// let mut set = UnorderedSet::new(b"n");
    ///
    /// assert_eq!(set.is_subset(&sup), true);
    /// set.insert("b".to_string());
    /// assert_eq!(set.is_subset(&sup), true);
    /// set.insert("d".to_string());
    /// assert_eq!(set.is_subset(&sup), false);
    /// ```
    pub fn is_subset(&self, other: &UnorderedSet<T, H>) -> bool
    where
        T: BorshDeserialize + Clone,
    {
        if self.len() <= other.len() {
            self.iter().all(|v| other.contains(v))
        } else {
            false
        }
    }

    /// Returns `true` if the set is a superset of another, i.e., `self` contains at least all the
    /// values in `other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedSet;
    ///
    /// let mut sub = UnorderedSet::new(b"m");
    /// sub.insert("a".to_string());
    /// sub.insert("b".to_string());
    ///
    /// let mut set = UnorderedSet::new(b"n");
    ///
    /// assert_eq!(set.is_superset(&sub), false);
    /// set.insert("b".to_string());
    /// set.insert("d".to_string());
    /// assert_eq!(set.is_superset(&sub), false);
    /// set.insert("a".to_string());
    /// assert_eq!(set.is_superset(&sub), true);
    /// ```
    pub fn is_superset(&self, other: &UnorderedSet<T, H>) -> bool
    where
        T: BorshDeserialize + Clone,
    {
        other.is_subset(self)
    }

    /// An iterator visiting all elements in arbitrary order.
    /// The iterator element type is `&'a T`.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedSet;
    ///
    /// let mut set = UnorderedSet::new(b"m");
    /// set.insert("a".to_string());
    /// set.insert("b".to_string());
    /// set.insert("c".to_string());
    ///
    /// for val in set.iter() {
    ///     println!("val: {}", val);
    /// }
    /// ```
    pub fn iter(&self) -> Iter<T>
    where
        T: BorshDeserialize,
    {
        Iter::new(self)
    }

    /// Clears the set, returning all elements in an iterator.
    ///
    /// # Examples
    ///
    /// ```
    /// use near_sdk::store::UnorderedSet;
    ///
    /// let mut a = UnorderedSet::new(b"m");
    /// a.insert(1);
    /// a.insert(2);
    ///
    /// for v in a.drain().take(1) {
    ///     assert!(v == 1 || v == 2);
    /// }
    ///
    /// assert!(a.is_empty());
    /// ```
    pub fn drain(&mut self) -> Drain<T, H>
    where
        T: BorshDeserialize,
    {
        Drain::new(self)
    }

    /// Returns `true` if the set contains the specified value.
    ///
    /// The value may be any borrowed form of the set's value type, but
    /// [`BorshSerialize`], [`ToOwned<Owned = T>`](ToOwned) and [`Ord`] on the borrowed form *must*
    /// match those for the value type.
    pub fn contains<Q: ?Sized>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: BorshSerialize + ToOwned<Owned = T> + Ord,
    {
        self.index.contains_key(value)
    }

    /// Adds a value to the set.
    ///
    /// If the set did not have this value present, true is returned.
    ///
    /// If the set did have this value present, false is returned.
    pub fn insert(&mut self, value: T) -> bool
    where
        T: Clone + BorshDeserialize,
    {
        let entry = self.index.get_mut_inner(&value);
        if entry.value_mut().is_some() {
            false
        } else {
            let element_index = self.elements.insert(value);
            entry.replace(Some(element_index));
            true
        }
    }

    /// Removes a value from the set. Returns whether the value was present in the set.
    ///
    /// The value may be any borrowed form of the set's value type, but
    /// [`BorshSerialize`], [`ToOwned<Owned = K>`](ToOwned) and [`Ord`] on the borrowed form *must*
    /// match those for the value type.
    ///
    /// # Performance
    ///
    /// When elements are removed, the underlying vector of values isn't
    /// rearranged; instead, the removed value is replaced with a placeholder value. These
    /// empty slots are reused on subsequent [`insert`](Self::insert) operations.
    ///
    /// In cases where there are a lot of removals and not a lot of insertions, these leftover
    /// placeholders might make iteration more costly, driving higher gas costs. If you need to
    /// remedy this, take a look at [`defrag`](Self::defrag).
    pub fn remove<Q: ?Sized>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q> + BorshDeserialize,
        Q: BorshSerialize + ToOwned<Owned = T> + Ord,
    {
        match self.index.remove(value) {
            Some(element_index) => {
                self.elements.remove(element_index).unwrap_or_else(|| {
                    env::panic_err(errors::InconsistentCollectionState::new().into())
                });
                true
            }
            None => false,
        }
    }

    /// Flushes the intermediate values of the map before this is called when the structure is
    /// [`Drop`]ed. This will write all modified values to storage but keep all cached values
    /// in memory.
    pub fn flush(&mut self) {
        self.elements.flush();
        self.index.flush();
    }
}

impl<T, H> UnorderedSet<T, H>
where
    T: BorshSerialize + BorshDeserialize + Ord,
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
    /// use near_sdk::store::UnorderedSet;
    ///
    /// let mut set = UnorderedSet::new(b"b");
    ///
    /// for i in 0..4 {
    ///     set.insert(i);
    /// }
    ///
    /// set.remove(&1);
    /// set.remove(&3);
    ///
    /// set.defrag();
    /// ```
    pub fn defrag(&mut self) {
        self.elements.defrag(|_, _| {});
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use crate::store::free_list::FreeListIndex;
    use crate::store::UnorderedSet;
    use crate::test_utils::test_env::setup_free;
    use arbitrary::{Arbitrary, Unstructured};
    use borsh::{to_vec, BorshDeserialize};
    use rand::RngCore;
    use rand::SeedableRng;
    use std::collections::HashSet;

    #[test]
    fn basic_functionality() {
        let mut set = UnorderedSet::new(b"b");
        assert!(set.is_empty());
        assert!(set.insert("test".to_string()));
        assert!(set.contains("test"));
        assert_eq!(set.len(), 1);

        assert!(set.remove("test"));
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn set_iterator() {
        let mut set = UnorderedSet::new(b"b");

        set.insert(0u8);
        set.insert(1);
        set.insert(2);
        set.insert(3);
        set.remove(&1);
        let iter = set.iter();
        assert_eq!(iter.len(), 3);
        assert_eq!(iter.collect::<Vec<_>>(), [(&0), (&2), (&3)]);

        let mut iter = set.iter();
        assert_eq!(iter.nth(2), Some(&3));
        // Check fused iterator assumption that each following one will be None
        assert_eq!(iter.next(), None);

        // Drain
        assert_eq!(set.drain().collect::<Vec<_>>(), [0, 2, 3]);
        assert!(set.is_empty());
    }

    #[test]
    fn test_drain() {
        let mut s = UnorderedSet::new(b"m");
        s.extend(1..100);

        // Drain the set a few times to make sure that it does have any random residue
        for _ in 0..20 {
            assert_eq!(s.len(), 99);

            for _ in s.drain() {}

            #[allow(clippy::never_loop)]
            for _ in &s {
                panic!("s should be empty!");
            }

            assert_eq!(s.len(), 0);
            assert!(s.is_empty());

            s.extend(1..100);
        }
    }

    #[test]
    fn test_extend() {
        let mut a = UnorderedSet::<u64>::new(b"m");
        a.insert(1);

        a.extend([2, 3, 4]);

        assert_eq!(a.len(), 4);
        assert!(a.contains(&1));
        assert!(a.contains(&2));
        assert!(a.contains(&3));
        assert!(a.contains(&4));
    }

    #[test]
    fn test_difference() {
        let mut set1 = UnorderedSet::new(b"m");
        set1.insert("a".to_string());
        set1.insert("b".to_string());
        set1.insert("c".to_string());
        set1.insert("d".to_string());

        let mut set2 = UnorderedSet::new(b"n");
        set2.insert("b".to_string());
        set2.insert("c".to_string());
        set2.insert("e".to_string());

        assert_eq!(
            set1.difference(&set2).collect::<HashSet<_>>(),
            ["a".to_string(), "d".to_string()].iter().collect::<HashSet<_>>()
        );
        assert_eq!(
            set2.difference(&set1).collect::<HashSet<_>>(),
            ["e".to_string()].iter().collect::<HashSet<_>>()
        );
        assert!(set1.difference(&set2).nth(1).is_some());
        assert!(set1.difference(&set2).nth(2).is_none());
    }

    #[test]
    fn test_difference_empty() {
        let mut set1 = UnorderedSet::new(b"m");
        set1.insert(1);
        set1.insert(2);
        set1.insert(3);

        let mut set2 = UnorderedSet::new(b"n");
        set2.insert(3);
        set2.insert(1);
        set2.insert(2);
        set2.insert(4);

        assert_eq!(set1.difference(&set2).collect::<HashSet<_>>(), HashSet::new());
    }

    #[test]
    fn test_symmetric_difference() {
        let mut set1 = UnorderedSet::new(b"m");
        set1.insert("a".to_string());
        set1.insert("b".to_string());
        set1.insert("c".to_string());

        let mut set2 = UnorderedSet::new(b"n");
        set2.insert("b".to_string());
        set2.insert("c".to_string());
        set2.insert("d".to_string());

        assert_eq!(
            set1.symmetric_difference(&set2).collect::<HashSet<_>>(),
            ["a".to_string(), "d".to_string()].iter().collect::<HashSet<_>>()
        );
        assert_eq!(
            set2.symmetric_difference(&set1).collect::<HashSet<_>>(),
            ["a".to_string(), "d".to_string()].iter().collect::<HashSet<_>>()
        );
    }

    #[test]
    fn test_symmetric_difference_empty() {
        let mut set1 = UnorderedSet::new(b"m");
        set1.insert(1);
        set1.insert(2);
        set1.insert(3);

        let mut set2 = UnorderedSet::new(b"n");
        set2.insert(3);
        set2.insert(1);
        set2.insert(2);

        assert_eq!(set1.symmetric_difference(&set2).collect::<HashSet<_>>(), HashSet::new());
    }

    #[test]
    fn test_intersection() {
        let mut set1 = UnorderedSet::new(b"m");
        set1.insert("a".to_string());
        set1.insert("b".to_string());
        set1.insert("c".to_string());

        let mut set2 = UnorderedSet::new(b"n");
        set2.insert("b".to_string());
        set2.insert("c".to_string());
        set2.insert("d".to_string());

        assert_eq!(
            set1.intersection(&set2).collect::<HashSet<_>>(),
            ["b".to_string(), "c".to_string()].iter().collect::<HashSet<_>>()
        );
        assert_eq!(
            set2.intersection(&set1).collect::<HashSet<_>>(),
            ["b".to_string(), "c".to_string()].iter().collect::<HashSet<_>>()
        );
        assert!(set1.intersection(&set2).nth(1).is_some());
        assert!(set1.intersection(&set2).nth(2).is_none());
    }

    #[test]
    fn test_intersection_empty() {
        let mut set1 = UnorderedSet::new(b"m");
        set1.insert(1);
        set1.insert(2);
        set1.insert(3);

        let mut set2 = UnorderedSet::new(b"n");
        set2.insert(4);
        set2.insert(6);
        set2.insert(5);

        assert_eq!(set1.intersection(&set2).collect::<HashSet<_>>(), HashSet::new());
    }

    #[test]
    fn test_union() {
        let mut set1 = UnorderedSet::new(b"m");
        set1.insert("a".to_string());
        set1.insert("b".to_string());
        set1.insert("c".to_string());

        let mut set2 = UnorderedSet::new(b"n");
        set2.insert("b".to_string());
        set2.insert("c".to_string());
        set2.insert("d".to_string());

        assert_eq!(
            set1.union(&set2).collect::<HashSet<_>>(),
            ["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string()]
                .iter()
                .collect::<HashSet<_>>()
        );
        assert_eq!(
            set2.union(&set1).collect::<HashSet<_>>(),
            ["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string()]
                .iter()
                .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn test_union_empty() {
        let set1 = UnorderedSet::<u64>::new(b"m");
        let set2 = UnorderedSet::<u64>::new(b"n");

        assert_eq!(set1.union(&set2).collect::<HashSet<_>>(), HashSet::new());
    }

    #[test]
    fn test_subset_and_superset() {
        let mut a = UnorderedSet::new(b"m");
        assert!(a.insert(0));
        assert!(a.insert(50));
        assert!(a.insert(110));
        assert!(a.insert(70));

        let mut b = UnorderedSet::new(b"n");
        assert!(b.insert(0));
        assert!(b.insert(70));
        assert!(b.insert(190));
        assert!(b.insert(2500));
        assert!(b.insert(110));
        assert!(b.insert(2000));

        assert!(!a.is_subset(&b));
        assert!(!a.is_superset(&b));
        assert!(!b.is_subset(&a));
        assert!(!b.is_superset(&a));

        assert!(b.insert(50));

        assert!(a.is_subset(&b));
        assert!(!a.is_superset(&b));
        assert!(!b.is_subset(&a));
        assert!(b.is_superset(&a));
    }

    #[test]
    fn test_disjoint() {
        let mut xs = UnorderedSet::new(b"m");
        let mut ys = UnorderedSet::new(b"n");

        assert!(xs.is_disjoint(&ys));
        assert!(ys.is_disjoint(&xs));

        assert!(xs.insert(50));
        assert!(ys.insert(110));
        assert!(xs.is_disjoint(&ys));
        assert!(ys.is_disjoint(&xs));

        assert!(xs.insert(70));
        assert!(xs.insert(190));
        assert!(xs.insert(40));
        assert!(ys.insert(20));
        assert!(ys.insert(-110));
        assert!(xs.is_disjoint(&ys));
        assert!(ys.is_disjoint(&xs));

        assert!(ys.insert(70));
        assert!(!xs.is_disjoint(&ys));
        assert!(!ys.is_disjoint(&xs));
    }

    #[derive(Arbitrary, Debug)]
    enum Op {
        Insert(u8),
        Remove(u8),
        Flush,
        Restore,
        Contains(u8),
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

            let mut us = UnorderedSet::new(b"l");
            let mut hs = HashSet::new();
            let u = Unstructured::new(&buf);
            if let Ok(ops) = Vec::<Op>::arbitrary_take_rest(u) {
                for op in ops {
                    match op {
                        Op::Insert(v) => {
                            let r1 = us.insert(v);
                            let r2 = hs.insert(v);
                            assert_eq!(r1, r2)
                        }
                        Op::Remove(v) => {
                            let r1 = us.remove(&v);
                            let r2 = hs.remove(&v);
                            assert_eq!(r1, r2)
                        }
                        Op::Flush => {
                            us.flush();
                        }
                        Op::Restore => {
                            let serialized = to_vec(&us).unwrap();
                            us = UnorderedSet::deserialize(&mut serialized.as_slice()).unwrap();
                        }
                        Op::Contains(v) => {
                            let r1 = us.contains(&v);
                            let r2 = hs.contains(&v);
                            assert_eq!(r1, r2)
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn defrag() {
        let mut set = UnorderedSet::new(b"b");

        let all_values = 0..=8;

        for i in all_values {
            set.insert(i);
        }

        let removed = [2, 4, 6];
        let existing = [0, 1, 3, 5, 7, 8];

        for id in removed {
            set.remove(&id);
        }

        set.defrag();

        for i in removed {
            assert!(!set.contains(&i));
        }
        for i in existing {
            assert!(set.contains(&i));
        }

        // Check that 8 and 7 moved from the front of the list to the smallest removed indices that
        // correspond to removed values.
        assert_eq!(*set.elements.get(FreeListIndex(2)).unwrap(), 8);
        assert_eq!(*set.elements.get(FreeListIndex(4)).unwrap(), 7);

        // Check the last removed value.
        assert_eq!(set.elements.get(FreeListIndex(6)), None);
    }
}
