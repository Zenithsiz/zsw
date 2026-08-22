//! Display

// Modules
pub mod geometry;
pub mod ser;

// Exports
pub use self::geometry::DisplayGeometry;

// Imports
use {
	core::str::FromStr,
	std::{borrow::Borrow, collections::BTreeMap, fmt, sync::Arc},
};

/// Displays
pub type Displays = BTreeMap<DisplayName, Arc<Display>>;

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

impl FromStr for DisplayName {
	type Err = !;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Arc::from(s)))
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
