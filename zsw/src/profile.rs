//! Profile

// Modules
mod profiles;
mod ser;

// Exports
pub use self::profiles::Profiles;

// Imports
use {
	crate::panel::PanelName,
	std::{borrow::Borrow, fmt, sync::Arc},
};

/// Profile
#[derive(Debug)]
pub struct Profile {
	/// Panels
	pub panels: Vec<PanelName>,
}

/// Profile name
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct ProfileName(Arc<str>);

impl From<String> for ProfileName {
	fn from(s: String) -> Self {
		Self(s.into())
	}
}

impl Borrow<str> for ProfileName {
	fn borrow(&self) -> &str {
		&self.0
	}
}

impl fmt::Display for ProfileName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

impl fmt::Debug for ProfileName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}
