//! Profiles

// TODO: Profile "inheritance".
//       This will likely require having a "current" profile,
//       so changes made only affect the child profile, and not
//       the parent?

// Features
#![feature(entry_insert)]

// Modules
mod profile;

// Exports
pub use profile::Profile;

// Imports
use {
	anyhow::Context,
	parking_lot::RwLock,
	std::{collections::HashMap, path::PathBuf, sync::Arc},
};

/// Profiles inner
#[derive(Debug)]
struct ProfilesInner {
	/// All profiles by their path
	profiles: HashMap<Arc<PathBuf>, Arc<Profile>>,
}

/// Profiles manager
#[derive(Clone, Debug)]
pub struct ProfilesManager {
	/// Inner
	inner: Arc<RwLock<ProfilesInner>>,
}

impl ProfilesManager {
	/// Returns all profiles by their path
	#[must_use]
	pub fn profiles(&self) -> Vec<(Arc<PathBuf>, Arc<Profile>)> {
		self.inner
			.read()
			.profiles
			.iter()
			.map(|(path, profile)| (Arc::clone(path), Arc::clone(profile)))
			.collect()
	}

	/// Adds a a new profiles
	fn create_new(&self, path: PathBuf, profile: Profile) -> Arc<Profile> {
		let path = Arc::new(path);
		let profile = Arc::new(profile);

		self.inner
			.write()
			.profiles
			.entry(path)
			.insert_entry(profile)
			.get()
			.clone()
	}

	/// Loads a profile
	pub fn load(&self, path: PathBuf) -> Result<Arc<Profile>, anyhow::Error> {
		// Load the profile
		let profile = zsw_util::parse_json_from_file(&path).context("Unable to load profile")?;

		// Then add it
		Ok(self.create_new(path, profile))
	}

	/// Adds and saves a profile
	pub fn save(&self, path: PathBuf, profile: Profile) -> Result<Arc<Profile>, anyhow::Error> {
		// Try to save it
		zsw_util::serialize_json_to_file(&path, &profile).context("Unable to save profile")?;

		// Then add it
		Ok(self.create_new(path, profile))
	}
}

/// Creates the profiles service
#[must_use]
pub fn create() -> ProfilesManager {
	let inner = ProfilesInner {
		profiles: HashMap::new(),
	};
	let inner = Arc::new(RwLock::new(inner));

	ProfilesManager { inner }
}
