//! Profiles

// TODO: Profile "inheritance".
//       This will likely require having a "current" profile,
//       so changes made only affect the child profile, and not
//       the parent?

// Features
#![feature(entry_insert)]

// Imports
use {
	anyhow::Context,
	std::{
		collections::HashMap,
		path::{Path, PathBuf},
	},
	zsw_panels::{Panel, Panels, PanelsResource},
	zsw_playlist::{Playlist, PlaylistResource},
	zsw_util::{Resources, Services},
};

/// Profiles
// TODO: Recently loaded profiles
#[derive(Debug)]
#[allow(missing_copy_implementations)] // It might not be `Copy` in the future
pub struct Profiles {}

#[allow(clippy::unused_self)] // For accessing resources, we should require the service
impl Profiles {
	/// Creates a new profiles, alongside the resources
	#[must_use]
	pub fn new() -> (Self, ProfilesResource) {
		(Self {}, ProfilesResource {
			profiles: HashMap::new(),
		})
	}

	/// Runs the initial profile loader and applier
	///
	/// # Lock
	/// [`ProfilesLock`]
	/// - [`zsw_playlist::PlaylistLock`]
	///   - [`zsw_panels::PanelsLock`]
	pub async fn run_loader_applier<S, R>(&self, path: &Path, services: &S, resources: &R)
	where
		S: Services<Playlist> + Services<Panels>,
		R: Resources<PanelsResource> + Resources<PlaylistResource> + Resources<ProfilesResource>,
	{
		let playlist = services.service::<Playlist>();
		let panels = services.service::<Panels>();

		// DEADLOCK: Caller ensures we can lock it
		let mut resource = resources.resource::<ProfilesResource>().await;

		// Then check if we got it
		match self.load(&mut resource, path.to_path_buf()) {
			// If we did, apply it
			Ok(profile) => {
				tracing::info!(?profile, "Successfully loaded profile");

				// Lock
				// DEADLOCK: Caller ensures we can lock them in this order after profiles lock
				let mut playlist_resource = resources.resource::<PlaylistResource>().await;
				let mut panels_resource = resources.resource::<PanelsResource>().await;

				// Then apply
				profile
					.apply(playlist, panels, &mut playlist_resource, &mut panels_resource)
					.await;
			},

			Err(err) => tracing::warn!(?err, "Unable to load profile"),
		}
	}

	/// Returns all profiles by their path
	#[must_use]
	pub fn profiles<'a>(&self, resource: &'a ProfilesResource) -> &'a HashMap<PathBuf, Profile> {
		&resource.profiles
	}

	/// Loads a profile
	pub fn load<'a>(&self, resource: &'a mut ProfilesResource, path: PathBuf) -> Result<&'a Profile, anyhow::Error> {
		// Try to load it
		let profile = zsw_util::parse_json_from_file(&path).context("Unable to load profile")?;

		// Then add it
		let profile = resource.profiles.entry(path).insert_entry(profile).into_mut();

		Ok(profile)
	}

	/// Adds and saves a profile
	pub fn save(&self, resource: &mut ProfilesResource, path: PathBuf, profile: Profile) -> Result<(), anyhow::Error> {
		// Try to save it
		zsw_util::serialize_json_to_file(&path, &profile).context("Unable to save profile")?;

		// Then add it
		#[allow(clippy::let_underscore_drop)] // We can drop the old profile
		let _ = resource.profiles.insert(path, profile);

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
		playlist_resource: &mut PlaylistResource,
		panels_resource: &mut PanelsResource,
	) {
		playlist.set_root_path(playlist_resource, self.root_path.clone()).await;
		panels.replace_panels(panels_resource, self.panels.iter().copied());
	}
}

/// Profiles resource
#[derive(Debug)]
pub struct ProfilesResource {
	/// All profiles by their path
	profiles: HashMap<PathBuf, Profile>,
}
