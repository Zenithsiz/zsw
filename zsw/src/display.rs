//! Display

// Modules
pub mod geometry;
pub mod ser;

// Exports
pub use self::geometry::DisplayGeometry;

// Imports
use {
	std::{borrow::Borrow, fmt, sync::Arc},
	zsw_util::Resources,
};

/// Displays
pub type Displays = Resources<DisplayName, Arc<Display>>;

/// Display
#[derive(Debug)]
#[derive(serde::Deserialize)]
#[serde(from = "ser::Display")]
pub struct Display {
	/// Geometries
	pub geometries: Vec<DisplayGeometry>,
}

impl From<ser::Display> for Display {
	fn from(display: ser::Display) -> Self {
		Self {
			geometries: display
				.geometries
				.into_iter()
				.map(|geometry| DisplayGeometry::new(geometry.geometry))
				.collect(),
		}
	}
}

/// Display name
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct DisplayName(Arc<str>);

impl From<String> for DisplayName {
	fn from(s: String) -> Self {
		Self(s.into())
	}
}

impl AsRef<str> for DisplayName {
	fn as_ref(&self) -> &str {
		&self.0
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
