//! Profiles

// Lints
#![warn(
	clippy::pedantic,
	clippy::nursery,
	missing_copy_implementations,
	missing_debug_implementations,
	noop_method_call,
	unused_results
)]
#![deny(
	// We want to annotate unsafe inside unsafe fns
	unsafe_op_in_unsafe_fn,
	// We muse use `expect` instead
	clippy::unwrap_used
)]
#![allow(
	// Style
	clippy::implicit_return,
	clippy::multiple_inherent_impl,
	clippy::pattern_type_mismatch,
	// `match` reads easier than `if / else`
	clippy::match_bool,
	clippy::single_match_else,
	//clippy::single_match,
	clippy::self_named_module_files,
	clippy::items_after_statements,
	clippy::module_name_repetitions,
	// Performance
	clippy::suboptimal_flops, // We prefer readability
	// Some functions might return an error in the future
	clippy::unnecessary_wraps,
	// Due to working with windows and rendering, which use `u32` / `f32` liberally
	// and interchangeably, we can't do much aside from casting and accepting possible
	// losses, although most will be lossless, since we deal with window sizes and the
	// such, which will fit within a `f32` losslessly.
	clippy::cast_precision_loss,
	clippy::cast_possible_truncation,
	// We use proper error types when it matters what errors can be returned, else,
	// such as when using `anyhow`, we just assume the caller won't check *what* error
	// happened and instead just bubbles it up
	clippy::missing_errors_doc,
	// Too many false positives and not too important
	clippy::missing_const_for_fn,
	// This is a binary crate, so we don't expose any API
	rustdoc::private_intra_doc_links,
)]

// Imports
use {
	anyhow::Context,
	parking_lot::Mutex,
	std::{
		collections::HashMap,
		path::{Path, PathBuf},
	},
	zsw_panels::{Panel, Panels},
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
	pub async fn apply<'playlist>(
		&self,
		playlist: &'playlist Playlist,
		panels: &Panels,
		playlist_lock: &mut zsw_playlist::PlaylistLock<'playlist>,
	) {
		playlist.set_root_path(playlist_lock, self.root_path.clone()).await;
		panels.replace_panels(self.panels.iter().copied());
	}
}
