use std::iter::FusedIterator;

use borsh::{BorshDeserialize, BorshSerialize};

use super::{FreeList, Slot};
use crate::{env, errors, store::vec};

impl<'a, T> IntoIterator for &'a FreeList<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut FreeList<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

fn decrement_count(count: &mut u32) {
    *count = count
        .checked_sub(1)
        .unwrap_or_else(|| env::panic_err(errors::InconsistentCollectionState::new().into()));
}

/// An iterator over elements in the storage bucket. This only yields the occupied entries.
pub struct Iter<'a, T>
where
    T: BorshDeserialize + BorshSerialize,
{
    /// Values iterator which contains empty and filled cells.
    values: vec::Iter<'a, Slot<T>>,
    /// Amount of valid elements left to iterate.
    elements_left: u32,
}

impl<'a, T> Iter<'a, T>
where
    T: BorshDeserialize + BorshSerialize,
{
    pub(super) fn new(bucket: &'a FreeList<T>) -> Self {
        Self { values: bucket.elements.iter(), elements_left: bucket.occupied_count }
    }
}

impl<'a, T> Iterator for Iter<'a, T>
where
    T: BorshDeserialize + BorshSerialize,
{
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.elements_left == 0 {
            return None;
        }
        loop {
            match self.values.next() {
                Some(Slot::Empty { .. }) => continue,
                Some(Slot::Occupied(value)) => {
                    decrement_count(&mut self.elements_left);
                    return Some(value);
                }
                None => {
                    // This should never be hit, because if 0 occupied elements, should have
                    // returned before the loop
                    env::panic_err(errors::InconsistentCollectionState::new().into())
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let elements_left = self.elements_left as usize;
        (elements_left, Some(elements_left))
    }

    fn count(self) -> usize {
        self.elements_left as usize
    }
}

impl<'a, T> ExactSizeIterator for Iter<'a, T> where T: BorshSerialize + BorshDeserialize {}
impl<'a, T> FusedIterator for Iter<'a, T> where T: BorshSerialize + BorshDeserialize {}

impl<'a, T> DoubleEndedIterator for Iter<'a, T>
where
    T: BorshSerialize + BorshDeserialize,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.elements_left == 0 {
            return None;
        }
        loop {
            match self.values.next_back() {
                Some(Slot::Empty { .. }) => continue,
                Some(Slot::Occupied(value)) => {
                    decrement_count(&mut self.elements_left);
                    return Some(value);
                }
                None => {
                    // This should never be hit, because if 0 occupied elements, should have
                    // returned before the loop
                    env::panic_err(errors::InconsistentCollectionState::new().into())
                }
            }
        }
    }
}

/// An iterator over elements in the storage bucket. This only yields the occupied entries.
pub struct IterMut<'a, T>
where
    T: BorshDeserialize + BorshSerialize,
{
    /// Values iterator which contains empty and filled cells.
    values: vec::IterMut<'a, Slot<T>>,
    /// Amount of valid elements left to iterate.
    elements_left: u32,
}

impl<'a, T> IterMut<'a, T>
where
    T: BorshDeserialize + BorshSerialize,
{
    pub(super) fn new(bucket: &'a mut FreeList<T>) -> Self {
        Self { values: bucket.elements.iter_mut(), elements_left: bucket.occupied_count }
    }
}

impl<'a, T> Iterator for IterMut<'a, T>
where
    T: BorshDeserialize + BorshSerialize,
{
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.elements_left == 0 {
            return None;
        }
        loop {
            match self.values.next() {
                Some(Slot::Empty { .. }) => continue,
                Some(Slot::Occupied(value)) => {
                    decrement_count(&mut self.elements_left);
                    return Some(value);
                }
                None => {
                    // This should never be hit, because if 0 occupied elements, should have
                    // returned before the loop
                    env::panic_err(errors::InconsistentCollectionState::new().into())
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let elements_left = self.elements_left as usize;
        (elements_left, Some(elements_left))
    }

    fn count(self) -> usize {
        self.elements_left as usize
    }
}

impl<'a, T> ExactSizeIterator for IterMut<'a, T> where T: BorshSerialize + BorshDeserialize {}
impl<'a, T> FusedIterator for IterMut<'a, T> where T: BorshSerialize + BorshDeserialize {}

impl<'a, T> DoubleEndedIterator for IterMut<'a, T>
where
    T: BorshSerialize + BorshDeserialize,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.elements_left == 0 {
            return None;
        }
        loop {
            match self.values.next_back() {
                Some(Slot::Empty { .. }) => continue,
                Some(Slot::Occupied(value)) => {
                    decrement_count(&mut self.elements_left);
                    return Some(value);
                }
                None => {
                    // This should never be hit, because if 0 occupied elements, should have
                    // returned before the loop
                    env::panic_err(errors::InconsistentCollectionState::new().into())
                }
            }
        }
    }
}

/// A draining iterator for [`FreeList<T>`].
#[derive(Debug)]
pub struct Drain<'a, T>
where
    T: BorshSerialize + BorshDeserialize,
{
    /// Inner vector drain iterator
    inner: vec::Drain<'a, Slot<T>>,
    /// Number of elements left to remove.
    remaining_count: usize,
}

impl<'a, T> Drain<'a, T>
where
    T: BorshSerialize + BorshDeserialize,
{
    pub(crate) fn new(list: &'a mut FreeList<T>) -> Self {
        // All elements will be dropped on drain iterator being dropped, fine to pre-emptively
        // reset these fields since a mutable reference is kept to the FreeList.
        list.first_free = None;
        let remaining_count = core::mem::take(&mut list.occupied_count) as usize;
        Self { inner: list.elements.drain(..), remaining_count }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.remaining_count
    }
}

impl<'a, T> Iterator for Drain<'a, T>
where
    T: BorshSerialize + BorshDeserialize,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.remaining() == 0 {
                return None;
            }

            match self.inner.next()? {
                Slot::Occupied(v) => {
                    self.remaining_count -= 1;
                    return Some(v);
                }
                Slot::Empty { .. } => continue,
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.remaining();
        (remaining, Some(remaining))
    }

    fn count(self) -> usize {
        self.remaining()
    }
}

impl<'a, T> ExactSizeIterator for Drain<'a, T> where T: BorshSerialize + BorshDeserialize {}
impl<'a, T> FusedIterator for Drain<'a, T> where T: BorshSerialize + BorshDeserialize {}

impl<'a, T> DoubleEndedIterator for Drain<'a, T>
where
    T: BorshSerialize + BorshDeserialize,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            if self.remaining() == 0 {
                return None;
            }

            match self.inner.next_back()? {
                Slot::Occupied(v) => {
                    self.remaining_count -= 1;
                    return Some(v);
                }
                Slot::Empty { .. } => continue,
            }
        }
    }
}
