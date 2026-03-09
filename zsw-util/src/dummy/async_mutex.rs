//! Async mutex

// Imports
use core::{
	marker::PhantomData,
	ops::{Deref, DerefMut},
};

#[derive(Debug)]
pub struct Mutex<T>(PhantomData<T>);

impl<T> Mutex<T> {
	pub fn new(_value: T) -> Self {
		Self(PhantomData)
	}

	#[expect(clippy::unused_async, reason = "It's necessary for the type signature")]
	pub async fn lock(&self) -> MutexGuard<'_, T> {
		MutexGuard(PhantomData)
	}
}

#[derive(Debug)]
pub struct MutexGuard<'a, T>(PhantomData<&'a mut T>);

impl<'a, T> MutexGuard<'a, T> {
	pub fn map<U>(self, _f: impl FnOnce(&mut T) -> &mut U) -> MutexGuard<'a, U> {
		MutexGuard(PhantomData)
	}
}

impl<T> Deref for MutexGuard<'_, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		crate::zst_ref_mut()
	}
}

impl<T> DerefMut for MutexGuard<'_, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		crate::zst_ref_mut()
	}
}
