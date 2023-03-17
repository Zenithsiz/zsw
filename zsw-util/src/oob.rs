//! Owned-Or-Borrowed

use std::{borrow::Borrow, ops::Deref};

/// Owned-Or-Borrowed.
///
/// Wraps a value that may be either owned, or borrowed
#[derive(Eq, Clone, Copy, Debug)]
pub enum Oob<'a, T> {
	/// Owned value
	Owned(T),

	/// Borrowed value
	Borrowed(&'a T),
}

impl<T> Oob<'_, T> {
	/// Returns a borrowed oob
	pub fn to_borrowed(&self) -> Oob<'_, T> {
		Oob::Borrowed(&**self)
	}
}

impl<T: PartialOrd> PartialOrd for Oob<'_, T> {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		(**self).partial_cmp(&**other)
	}
}

impl<T: Ord> Ord for Oob<'_, T> {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		(**self).cmp(&**other)
	}
}

impl<T: PartialEq> PartialEq for Oob<'_, T> {
	fn eq(&self, other: &Self) -> bool {
		**self == **other
	}
}

impl<T> Deref for Oob<'_, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		match self {
			Oob::Owned(value) => value,
			Oob::Borrowed(value) => value,
		}
	}
}

impl<T> AsRef<T> for Oob<'_, T> {
	fn as_ref(&self) -> &T {
		self
	}
}

impl<T> Borrow<T> for Oob<'_, T> {
	fn borrow(&self) -> &T {
		self
	}
}
