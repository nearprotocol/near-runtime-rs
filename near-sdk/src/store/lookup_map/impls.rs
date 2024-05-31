use std::borrow::Borrow;

use borsh::{BorshDeserialize, BorshSerialize};

use super::{LookupMap, ToKey};
use crate::{env, errors};

impl<K, V, H> Extend<(K, V)> for LookupMap<K, V, H>
where
    K: BorshSerialize + Ord,
    V: BorshSerialize,
    H: ToKey,
{
    fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (K, V)>,
    {
        for (key, value) in iter {
            self.set(key, Some(value))
        }
    }
}

impl<K, V, H, Q: ?Sized> core::ops::Index<&Q> for LookupMap<K, V, H>
where
    K: BorshSerialize + Ord + Borrow<Q>,
    V: BorshSerialize + BorshDeserialize,
    H: ToKey,

    Q: BorshSerialize + ToOwned<Owned = K>,
{
    type Output = V;

    /// Returns reference to value corresponding to key.
    ///
    /// # Panics
    ///
    /// Panics if the key does not exist in the map
    fn index(&self, index: &Q) -> &Self::Output {
        self.get(index).unwrap_or_else(|| env::panic_err(errors::KeyNotFound {}.into()))
    }
}
