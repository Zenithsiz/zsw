//! Profiles

use std::path::Path;

use zsw_side_effect_macros::side_effect;

// Imports
use {
	crate::{
		util,
		util::{extse::ParkingLotMutexSe, MightBlock},
		Panel,
	},
	anyhow::Context,
	parking_lot::Mutex,
	std::{collections::HashMap, path::PathBuf},
};

/// Profiles
#[derive(Debug)]
pub struct Profiles {
	/// All profiles by their path
	profiles: Mutex<HashMap<PathBuf, Profile>>,
	// TODO: Recently loaded profiles
}

impl Profiles {
	/// Creates a new profiles
	pub fn new() -> Result<Self, anyhow::Error> {
		Ok(Self {
			profiles: Mutex::new(HashMap::new()),
		})
	}

	/// Loads a profile
	pub fn load(&self, path: PathBuf) -> Result<(), anyhow::Error> {
		// Try to load it
		let profile = util::parse_json_from_file(&path).context("Unable to load profile")?;

		// Then add it
		// DEADLOCK: We only lock it internally and we don't block while locked
		#[allow(clippy::let_underscore_drop)] // We can drop the old profile
		let _ = self.profiles.lock_se().allow::<MightBlock>().insert(path, profile);

		Ok(())
	}

	/// Adds and saves a profile
	pub fn save(&self, path: PathBuf, profile: Profile) -> Result<(), anyhow::Error> {
		// Try to save it
		util::serialize_json_to_file(&path, &profile).context("Unable to save profile")?;

		// Then add it
		#[allow(clippy::let_underscore_drop)] // We can drop the old profile
		let _ = self.profiles.lock_se().allow::<MightBlock>().insert(path, profile);

		Ok(())
	}

	/// Iterates over all profiles
	///
	/// # Blocking
	/// Will deadlock if `f` blocks.
	#[side_effect(MightBlock)]
	pub fn for_each<T, C: FromIterator<T>>(&self, mut f: impl FnMut(&Path, &Profile) -> T) -> C {
		// DEADLOCK: We only lock it internally and caller guarantees it won't block
		//           Caller ensures `f` won't block
		let profiles = self.profiles.lock_se().allow::<MightBlock>();
		profiles.iter().map(|(path, profile)| f(path, profile)).collect()
	}
}


/// A profile
#[derive(Clone, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Profile {
	/// Root path
	pub root_path: PathBuf,

	/// All panels
	pub panels: Vec<Panel>,
}
