//! Collections and types used when interacting with storage.
//!
//! These collections are more scalable versions of [`std::collections`] when used as contract
//! state because it allows values to be lazily loaded and stored based on what is actually
//! interacted with.
//!
//! Fundamentally, a contract's storage is a key/value store where both keys and values are just
//! [`Vec<u8>`]. If you want to store some structured data, for example, [`Vec<Account>`], one way
//! to achieve that would be to serialize the Vec to bytes and store that. This has a drawback in
//! that accessing or modifying a single element would require reading the whole `Vec` from the
//! storage.
//!
//! That's where `store` module helps. Its collections are backed by a key value store.
//! For example, a store::Vector is stored as several key-value pairs, where indices are the keys.
//! So, accessing a single element would only load this specific element.
//!
//! It can be expensive to load all values into memory, and because of this, `serde`
//! [`Serialize`](serde::Serialize) and [`Deserialize`](serde::Deserialize) traits are
//! intentionally not implemented. If you want to return all values from a storage collection from
//! a function, consider using pagination with the collection iterators.
//!
//! All of the collections implement [`BorshSerialize`](borsh::BorshSerialize) and
//! [`BorshDeserialize`](borsh::BorshDeserialize) to be able to store the metadata of the
//! collections to be able to access all values. Because only metadata is serialized, these
//! structures should not be used as a borsh return value from a function.
//!
//! The collections are as follows:
//!
//! Sequences:
//!
//! - [`Vector`]: Analogous to [`Vec`] but not contiguous and persisted to storage.
//!
//! Maps:
//!
//! - [`LookupMap`]: Wrapper around key-value storage interactions, similar to
//! [`UnorderedMap`]/[`std::collections::HashMap`] except that keys are not persisted and cannot be
//! iterated over.
//!
//! - [`UnorderedMap`]: Storage version of [`std::collections::HashMap`]. No ordering
//! guarantees.
//!
//! - [`TreeMap`](TreeMap) (`unstable`): Storage version of [`std::collections::BTreeMap`]. Ordered by key,
//! which comes at the cost of more expensive lookups and iteration.
//!
//! Sets:
//!
//! - [`LookupSet`]: Non-iterable storage version of [`std::collections::HashSet`].
//!
//! - [`UnorderedSet`]: Analogous to [`std::collections::HashSet`], and is an iterable
//! version of [`LookupSet`] and persisted to storage.
//!
//! Basic Types:
//!
//! - [`Lazy<T>`](Lazy): Lazily loaded type that can be used in place of a type `T`.
//! Will only be loaded when interacted with and will persist on [`Drop`].
//!
//! - [`LazyOption<T>`](LazyOption): Lazily loaded, optional type that can be used in
//! place of a type [`Option<T>`](Option). Will only be loaded when interacted with and will
//! persist on [`Drop`].

#[cfg(feature = "unstable")]
mod lazy;
#[cfg(feature = "unstable")]
pub use lazy::Lazy;

#[cfg(feature = "unstable")]
mod lazy_option;
#[cfg(feature = "unstable")]
pub use lazy_option::LazyOption;

pub mod vec;
pub use vec::Vector;

pub mod lookup_map;
pub use self::lookup_map::LookupMap;

mod lookup_set;
pub use self::lookup_set::LookupSet;

pub mod iterable_map;
pub use self::iterable_map::IterableMap;
pub mod iterable_set;
pub use self::iterable_set::IterableSet;
pub mod unordered_map;
#[allow(deprecated)]
pub use self::unordered_map::UnorderedMap;

pub mod unordered_set;
#[allow(deprecated)]
pub use self::unordered_set::UnorderedSet;

#[cfg(feature = "unstable")]
pub mod tree_map;
#[cfg(feature = "unstable")]
pub use self::tree_map::TreeMap;

mod index_map;
pub(crate) use self::index_map::IndexMap;

pub(crate) mod free_list;
pub(crate) use self::free_list::FreeList;

/// Storage key hash function types and trait to override map hash functions.
pub mod key;
