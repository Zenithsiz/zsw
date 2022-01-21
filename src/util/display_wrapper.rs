//! Display wrapper.

// Imports
use std::{cell::RefCell, fmt};

/// A display wrapper using `F`
pub struct DisplayWrapper<F: FnMut(&mut fmt::Formatter) -> fmt::Result>(RefCell<F>);

impl<F: FnMut(&mut fmt::Formatter) -> fmt::Result> DisplayWrapper<F> {
	/// Creates a new display wrapper
	#[must_use]
	pub fn new(func: F) -> Self {
		Self(RefCell::new(func))
	}
}


impl<F: FnMut(&mut fmt::Formatter) -> fmt::Result> fmt::Display for DisplayWrapper<F> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		// Note: `f` cannot be re-entrant, so this cannot fail
		self.0.borrow_mut()(f)
	}
}
