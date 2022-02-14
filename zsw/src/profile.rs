//! Profiles

// Imports
use {
	crate::{Panel, Panels},
	anyhow::Context,
	parking_lot::Mutex,
	std::{
		collections::HashMap,
		path::{Path, PathBuf},
	},
	zsw_playlist::Playlist,
	zsw_side_effect_macros::side_effect,
	zsw_util::{extse::ParkingLotMutexSe, MightBlock},
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
	pub fn load(&self, path: PathBuf) -> Result<Profile, anyhow::Error> {
		// Try to load it
		let profile = zsw_util::parse_json_from_file(&path).context("Unable to load profile")?;

		// Then add it
		// TODO: Maybe don't clone?
		// DEADLOCK: We only lock it internally and we don't block while locked
		#[allow(clippy::let_underscore_drop)] // We can drop the old profile
		let profile = self
			.profiles
			.lock_se()
			.allow::<MightBlock>()
			.entry(path)
			.insert_entry(profile)
			.get()
			.clone();

		Ok(profile)
	}

	/// Adds and saves a profile
	pub fn save(&self, path: PathBuf, profile: Profile) -> Result<(), anyhow::Error> {
		// Try to save it
		zsw_util::serialize_json_to_file(&path, &profile).context("Unable to save profile")?;

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

impl Profile {
	/// Applies a profile
	pub async fn apply(&self, playlist: &Playlist, panels: &Panels) {
		playlist.set_root_path(self.root_path.clone()).await;
		panels.replace_panels(self.panels.iter().copied());
	}
}
