//! Profiles

// TODO: Profile "inheritance".
//       This will likely require having a "current" profile,
//       so changes made only affect the child profile, and not
//       the parent?

// Features
#![feature(entry_insert)]
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
	futures::lock::{Mutex, MutexGuard},
	std::{
		collections::HashMap,
		path::{Path, PathBuf},
	},
	zsw_panels::{Panel, Panels},
	zsw_playlist::Playlist,
	zsw_side_effect_macros::side_effect,
	zsw_util::{extse::AsyncLockMutexSe, MightBlock},
};

/// Profiles
// TODO: Recently loaded profiles
#[derive(Debug)]
pub struct Profiles {
	/// All profiles by their path
	profiles: Mutex<HashMap<PathBuf, Profile>>,

	/// Lock source
	lock_source: LockSource,
}

impl Profiles {
	/// Creates a new profiles
	pub fn new() -> Result<Self, anyhow::Error> {
		Ok(Self {
			profiles:    Mutex::new(HashMap::new()),
			lock_source: LockSource,
		})
	}

	/// Creates a profiles lock
	///
	/// # Blocking
	/// Will block until any existing profiles locks are dropped
	#[side_effect(MightBlock)]
	pub async fn lock_profiles<'a>(&'a self) -> ProfilesLock<'a> {
		// DEADLOCK: Caller is responsible to ensure we don't deadlock
		//           We don't lock it outside of this method
		let guard = self.profiles.lock_se().await.allow::<MightBlock>();
		ProfilesLock::new(guard, &self.lock_source)
	}

	/// Runs the initial profile loader and applier
	///
	/// # Lock
	/// [`ProfilesLock`]
	/// - [`zsw_playlist::PlaylistLock`]
	///   - [`zsw_panels::PanelsLock`]
	#[side_effect(MightBlock)]
	pub async fn run_loader_applier<'profiles, 'playlist, 'panels>(
		&'profiles self,
		path: &Path,
		playlist: &'playlist Playlist,
		panels: &'panels Panels,
	) {
		// DEADLOCK: Caller ensures we can lock it
		let mut profiles_lock = self.lock_profiles().await.allow::<MightBlock>();

		// Then check if we got it
		match self.load(&mut profiles_lock, path.to_path_buf()) {
			// If we did, apply it
			Ok(profile) => {
				log::info!("Successfully loaded profile: {profile:?}");

				// Lock
				// DEADLOCK: Caller ensures we can lock them in this order after profiles lock
				let mut playlist_lock = playlist.lock_playlist().await.allow::<MightBlock>();
				let mut panels_lock = panels.lock_panels().await.allow::<MightBlock>();

				// Then apply
				profile
					.apply(playlist, panels, &mut playlist_lock, &mut panels_lock)
					.await;
			},

			Err(err) => log::warn!("Unable to load profile: {err:?}"),
		}
	}

	/// Returns all profiles by their path
	pub fn profiles<'a>(&self, profiles_lock: &'a ProfilesLock) -> &'a HashMap<PathBuf, Profile> {
		profiles_lock.get(&self.lock_source)
	}

	/// Loads a profile
	pub fn load<'a>(&self, profiles_lock: &'a mut ProfilesLock, path: PathBuf) -> Result<&'a Profile, anyhow::Error> {
		// Try to load it
		let profile = zsw_util::parse_json_from_file(&path).context("Unable to load profile")?;

		// Then add it
		let profile = profiles_lock
			.get_mut(&self.lock_source)
			.entry(path)
			.insert_entry(profile)
			.into_mut();

		Ok(profile)
	}

	/// Adds and saves a profile
	pub fn save(&self, profiles_lock: &mut ProfilesLock, path: PathBuf, profile: Profile) -> Result<(), anyhow::Error> {
		// Try to save it
		zsw_util::serialize_json_to_file(&path, &profile).context("Unable to save profile")?;

		// Then add it
		#[allow(clippy::let_underscore_drop)] // We can drop the old profile
		let _ = profiles_lock.get_mut(&self.lock_source).insert(path, profile);

		Ok(())
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
	pub async fn apply<'playlist, 'panels>(
		&self,
		playlist: &'playlist Playlist,
		panels: &'panels Panels,
		playlist_lock: &mut zsw_playlist::PlaylistLock<'playlist>,
		panels_lock: &mut zsw_panels::PanelsLock<'panels>,
	) {
		playlist.set_root_path(playlist_lock, self.root_path.clone()).await;
		panels.replace_panels(panels_lock, self.panels.iter().copied());
	}
}

/// Source for all locks
// Note: This is to ensure user can't create the locks themselves
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct LockSource;

/// Profiles lock
pub type ProfilesLock<'a> = zsw_util::Lock<'a, MutexGuard<'a, HashMap<PathBuf, Profile>>, LockSource>;
