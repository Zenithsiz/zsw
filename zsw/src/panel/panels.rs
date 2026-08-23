//! Panels

// Imports
use {
	super::Panel,
	crate::{
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
		},
	},
	app_error::Context,
	std::sync::{
		Arc,
		nonpoison::{MappedMutexGuard, Mutex, MutexGuard},
	},
	zsw_util::{AppError, WalkDir},
	zutil_cloned::cloned,
};

/// Inner
#[derive(Debug)]
struct Inner {
	/// Profile name
	profile_name: Option<ProfileName>,

	/// Panels
	panels: Vec<Panel>,
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
				profile_name: None,
				panels:       vec![],
			}),
		}
	}

	/// Gets the panels
	pub fn get_all(&self) -> MappedMutexGuard<'_, [Panel]> {
		MutexGuard::map(self.inner.lock(), |inner| inner.panels.as_mut_slice())
	}

	/// Sets the current profile.
	///
	/// If a profile already exists, unloads it's panels first
	pub fn set_profile(
		&self,
		profile_name: ProfileName,
		profile: &Profile,
		playlists: &Arc<Playlists>,
	) -> Result<(), AppError> {
		let mut inner = self.inner.lock();
		inner.profile_name = Some(profile_name);
		inner.panels.clear();
		for profile_panel in &profile.panels {
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
					zsw_util::spawn_task(
						format!("Load panel {:?} playlists", profile_panel.display_name),
						move || {
							for playlist_name in panel_playlists {
								self::load_playlist(&playlist_player, &playlist_name, &playlists)
									.with_context(|| format!("Unable to load playlist {playlist_name:?}"))?;
							}

							Ok(())
						},
					);

					PanelState::Fade(state)
				},
				ProfilePanelShader::Slide(shader) => {
					let state = PanelSlideState::new(match shader.inner {
						ProfilePanelSlideShaderInner::Basic => PanelSlideShader::Basic,
					});

					PanelState::Slide(state)
				},
			};

			let panel = Panel::new(profile_panel.display_name.clone(), state);
			inner.panels.push(panel);
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
	let playlist = playlists.get(playlist_name).context("Unable to load playlist")?;

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

				let dir = match builder.build(dir_path.as_path()) {
					Ok(dir) => dir,
					Err(err) => {
						let err = AppError::new(&err);
						tracing::warn!("Unable to read directory {dir_path:?}: {err:?}");
						continue;
					},
				};

				for entry in dir {
					let entry = match entry {
						Ok(entry) => entry,
						Err(err) => {
							let err = AppError::new(&err);
							tracing::warn!("Unable to read directory entry: {err:?}");
							continue;
						},
					};

					let file_type = match entry.file_type() {
						Ok(file_type) => file_type,
						Err(err) => {
							let err = AppError::new(&err);
							tracing::warn!("Unable to read directory entry file: {err:?}");
							continue;
						},
					};

					if !file_type.is_dir() {
						playlist_player.lock().insert(entry.path().into());
					}
				}
			},
			PlaylistItemKind::File { ref path } => playlist_player.lock().insert(Arc::clone(path)),
		}
	}

	Ok(())
}
