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

impl<'a, T> Oob<'a, T> {
	/// Returns a borrowed oob
	pub fn to_borrowed(&self) -> Oob<'_, T> {
		Oob::Borrowed(&**self)
	}
}

impl<'a, T: PartialOrd> PartialOrd for Oob<'a, T> {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		(**self).partial_cmp(&**other)
	}
}

impl<'a, T: Ord> Ord for Oob<'a, T> {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		(**self).cmp(&**other)
	}
}

impl<'a, T: PartialEq> PartialEq for Oob<'a, T> {
	fn eq(&self, other: &Self) -> bool {
		**self == **other
	}
}

impl<'a, T> Deref for Oob<'a, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		match self {
			Oob::Owned(value) => value,
			Oob::Borrowed(value) => value,
		}
	}
}

impl<'a, T> AsRef<T> for Oob<'a, T> {
	fn as_ref(&self) -> &T {
		self
	}
}

impl<'a, T> Borrow<T> for Oob<'a, T> {
	fn borrow(&self) -> &T {
		self
	}
}
