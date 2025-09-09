//! Display

// Modules
mod displays;
pub mod ser;

// Exports
pub use self::displays::Displays;

// Imports
use {
	std::{borrow::Borrow, fmt, sync::Arc},
	zsw_util::Rect,
};

/// Display
#[derive(Debug)]
pub struct Display {
	/// Name
	pub name: DisplayName,

	/// Geometries
	pub geometries: Vec<Rect<i32, u32>>,
}

/// Display name
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct DisplayName(Arc<str>);

impl From<String> for DisplayName {
	fn from(s: String) -> Self {
		Self(s.into())
	}
}

impl Borrow<str> for DisplayName {
	fn borrow(&self) -> &str {
		&self.0
	}
}

impl fmt::Display for DisplayName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

impl fmt::Debug for DisplayName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}
