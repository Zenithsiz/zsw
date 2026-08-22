//! Panels

// Imports
use {
	super::Panel,
	crate::{
		display::Displays,
		panel::{
			PanelFadeShader,
			PanelSlideShader,
			PanelState,
			state::{PanelFadeState, PanelNoneState, PanelSlideState},
		},
		playlist::{PlaylistItemKind, PlaylistName, PlaylistPlayer, Playlists},
		profile::{
			Profile,
			ProfileName,
			ProfilePanelFadeShaderInner,
			ProfilePanelShader,
			ProfilePanelSlideShaderInner,
			Profiles,
		},
	},
	app_error::Context,
	std::{
		fs,
		sync::{
			Arc,
			nonpoison::{Mutex, RwLock},
		},
	},
	zsw_util::{AppError, WalkDir},
	zutil_cloned::cloned,
};

/// Inner
#[derive(Debug)]
struct Inner {
	/// Profile
	profile: Option<Arc<RwLock<Profile>>>,

	/// Panels
	panels: Vec<Arc<Mutex<Panel>>>,
}

/// Panels
#[derive(Debug)]
pub struct Panels {
	/// Inner
	inner: Mutex<Inner>,
}

impl Panels {
	/// Creates the panels with no current profile
	pub fn new() -> Self {
		Self {
			inner: Mutex::new(Inner {
				profile: None,
				panels:  vec![],
			}),
		}
	}

	/// Gets all of the panels
	pub fn get_all(&self) -> Vec<Arc<Mutex<Panel>>> {
		self.inner.lock().panels.clone()
	}

	/// Sets the current profile.
	///
	/// If a profile already exists, unloads it's panels first
	pub fn set_profile(
		&self,
		profile_name: &ProfileName,
		displays: &Displays,
		playlists: &Arc<Playlists>,
		profiles: &Profiles,
	) -> Result<(), AppError> {
		// Get the new profile
		let profile = profiles.load(profile_name.clone()).context("Unable to load profile")?;

		// If we have a previous loaded profile, clear all panels before proceeding
		{
			let mut inner = self.inner.lock();
			if let Some(old_profile) = &inner.profile {
				tracing::info!("Dropping previous profile: {:?}", old_profile.read().name);
				inner.panels.clear();
			}
			inner.profile = Some(Arc::clone(&profile));
			tracing::info!("Setting current profile: {profile_name:?}");
		}

		// Then load it's panels
		for profile_panel in &profile.read().panels {
			let display = displays
				.load(profile_panel.display.clone())
				.with_context(|| format!("Unable to load display {:?}", profile_panel.display))?;


			let state = match &profile_panel.shader {
				ProfilePanelShader::None(shader) => PanelState::None(PanelNoneState::new(shader.background_color)),
				ProfilePanelShader::Fade(shader) => {
					let state = PanelFadeState::new(shader.duration, shader.fade_duration, match shader.inner {
						ProfilePanelFadeShaderInner::Basic => PanelFadeShader::Basic,
						ProfilePanelFadeShaderInner::White { strength } => PanelFadeShader::White { strength },
						ProfilePanelFadeShaderInner::Out { strength } => PanelFadeShader::Out { strength },
						ProfilePanelFadeShaderInner::In { strength } => PanelFadeShader::In { strength },
					});

					#[cloned(playlists, panel_playlists = shader.playlists, playlist_player = state.playlist_player())]
					zsw_util::spawn_task(format!("Load panel {:?} playlists", profile_panel.display), move || {
						for playlist_name in panel_playlists {
							self::load_playlist(&playlist_player, &playlist_name, &playlists)
								.with_context(|| format!("Unable to load playlist {playlist_name:?}"))?;
						}

						Ok(())
					});

					PanelState::Fade(state)
				},
				ProfilePanelShader::Slide(shader) => {
					let state = PanelSlideState::new(match shader.inner {
						ProfilePanelSlideShaderInner::Basic => PanelSlideShader::Basic,
					});

					PanelState::Slide(state)
				},
			};

			let panel = Panel::new(display, state);
			self.inner.lock().panels.push(Arc::new(Mutex::new(panel)));
		}

		Ok(())
	}
}

/// Loads a panel's playlist
fn load_playlist(
	playlist_player: &Mutex<PlaylistPlayer>,
	playlist_name: &PlaylistName,
	playlists: &Playlists,
) -> Result<(), AppError> {
	let playlist = playlists
		.load(playlist_name.clone())
		.context("Unable to load playlist")?;
	let playlist = playlist.read();

	for item in &playlist.items {
		// If not enabled, skip it
		if !item.enabled {
			continue;
		}

		// Else check the kind of item
		match item.kind {
			PlaylistItemKind::Directory {
				path: ref dir_path,
				follow_symlinks,
				recursive,
			} => {
				let builder = WalkDir::builder()
					.recurse_symlink(follow_symlinks)
					.max_depth(match recursive {
						true => None,
						false => Some(1),
					});

				for entry in builder.build(dir_path.as_path()) {
					let entry = match entry {
						Ok(entry) => entry,
						Err(err) => {
							let err = AppError::new(&err);
							tracing::warn!("Unable to read directory entry: {}", err.pretty());
							continue;
						},
					};

					let file_type = match entry.file_type() {
						Ok(file_type) => file_type,
						Err(err) => {
							let err = AppError::new(&err);
							tracing::warn!("Unable to read directory entry file: {}", err.pretty());
							continue;
						},
					};

					if !file_type.is_dir() {
						playlist_player.lock().insert(entry.path().into());
					}
				}
			},
			PlaylistItemKind::File { ref path } => match fs::canonicalize(path) {
				Ok(path) => playlist_player.lock().insert(path.into()),
				Err(err) => {
					let err = AppError::new(&err);
					tracing::warn!("Unable to canonicalize playlist entry {path:?}: {}", err.pretty());
				},
			},
		}
	}

	Ok(())
}
