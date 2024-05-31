use borsh::{BorshDeserialize, BorshSerialize};

use super::iter::{Iter, IterMut};
use super::Vector;
use crate::{env, errors};

impl<T> Drop for Vector<T>
where
    T: BorshSerialize,
{
    fn drop(&mut self) {
        self.flush()
    }
}

impl<'a, T> IntoIterator for &'a Vector<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Vector<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T> Extend<T> for Vector<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        for item in iter {
            self.push(item)
        }
    }
}

impl<T> core::ops::Index<u32> for Vector<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    type Output = T;

    fn index(&self, index: u32) -> &Self::Output {
        self.get(index).unwrap_or_else(|| env::panic_err(errors::IndexOutOfBounds {}.into()))
    }
}

impl<T> core::ops::IndexMut<u32> for Vector<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    fn index_mut(&mut self, index: u32) -> &mut Self::Output {
        self.get_mut(index).unwrap_or_else(|| env::panic_err(errors::IndexOutOfBounds {}.into()))
    }
}
