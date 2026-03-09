//! Hash map

// Imports
use core::marker::PhantomData;

/// Dummy hashmap
#[derive(Clone, Debug)]
pub struct HashMap<K, V>(PhantomData<(K, V)>);

impl<K, V> HashMap<K, V> {
	#[must_use]
	pub fn new() -> Self {
		Self(PhantomData)
	}

	pub fn entry(&mut self, _key: K) -> Entry<'_, K, V> {
		Entry::Vacant(VacantEntry(PhantomData))
	}

	pub fn insert(&mut self, _key: K, _value: V) -> Option<V> {
		None
	}

	pub fn keys(&self) -> impl Iterator<Item = &K> {
		[].into_iter()
	}
}

impl<K, V> Default for HashMap<K, V> {
	fn default() -> Self {
		Self::new()
	}
}

#[derive(Debug)]
pub enum Entry<'a, K, V> {
	Occupied(OccupiedEntry<'a, K, V>),
	Vacant(VacantEntry<'a, K, V>),
}

impl<'a, K, V> Entry<'a, K, V> {
	pub fn or_default(&mut self) -> &'a mut V {
		const { crate::zst_ref_mut::<'a, V>() }
	}

	pub fn or_insert_with(&mut self, _f: impl FnOnce() -> V) -> &'a mut V {
		const { crate::zst_ref_mut::<'a, V>() }
	}
}

#[derive(Debug)]
pub struct OccupiedEntry<'a, K, V>(PhantomData<&'a mut HashMap<K, V>>);

#[derive(Debug)]
pub struct VacantEntry<'a, K, V>(PhantomData<&'a mut HashMap<K, V>>);

impl<'a, K, V> VacantEntry<'a, K, V> {
	pub fn insert(self, _value: V) -> &'a mut V {
		const { crate::zst_ref_mut::<'a, V>() }
	}
}
