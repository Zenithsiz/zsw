//! Panels

// Imports
use {
	super::Panel,
	crate::{
		display::Displays,
		panel::{PanelFadeShader, PanelFadeState, PanelNoneState, PanelSlideShader, PanelSlideState, PanelState},
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
	core::ops::DerefMut,
	futures::{StreamExt, TryStreamExt, stream::FuturesUnordered},
	std::sync::Arc,
	tokio::{
		fs,
		sync::{Mutex, MutexGuard, RwLock},
	},
	zsw_util::{AppError, UnwrapOrReturnExt, WalkDir},
	zutil_cloned::cloned,
};

/// Inner
#[derive(Debug)]
struct Inner {
	/// Profile
	profile: Option<Arc<RwLock<Profile>>>,

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
				profile: None,
				panels:  vec![],
			}),
		}
	}

	/// Gets all of the panels
	pub async fn get_all(&self) -> impl DerefMut<Target = [Panel]> {
		MutexGuard::map(self.inner.lock().await, |inner| inner.panels.as_mut_slice())
	}

	/// Sets the current profile.
	///
	/// If a profile already exists, unloads it's panels first
	pub async fn set_profile(
		&self,
		profile_name: ProfileName,
		displays: &Displays,
		playlists: &Arc<Playlists>,
		profiles: &Profiles,
	) -> Result<(), AppError> {
		// Get the new profile
		let profile = profiles
			.load(profile_name.clone())
			.await
			.context("Unable to load profile")?;

		// If we have a previous loaded profile, clear all panels before proceeding
		{
			let mut inner = self.inner.lock().await;
			if let Some(old_profile) = &inner.profile {
				tracing::info!("Dropping previous profile: {:?}", old_profile.read().await.name);
				inner.panels.clear();
			}
			inner.profile = Some(Arc::clone(&profile));
			tracing::info!("Setting current profile: {profile_name:?}");
		}


		// Then load it's panels
		profile
			.read()
			.await
			.panels
			.iter()
			.map(async |profile_panel| {
				let display = displays
					.load(profile_panel.display.clone())
					.await
					.with_context(|| format!("Unable to load display {:?}", profile_panel.display))?;


				let panel_state = match &profile_panel.shader {
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
							format!("Load panel {:?} playlists", profile_panel.display),
							async move {
								panel_playlists
									.into_iter()
									.map(async |playlist_name| {
										self::load_playlist(&playlist_player, &playlist_name, &playlists)
											.await
											.with_context(|| format!("Unable to load playlist {playlist_name:?}"))
									})
									.collect::<FuturesUnordered<_>>()
									.try_collect::<()>()
									.await
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

				let panel = Panel::new(display, panel_state);
				self.inner.lock().await.panels.push(panel);

				Ok::<_, AppError>(())
			})
			.collect::<FuturesUnordered<_>>()
			.try_collect::<()>()
			.await?;

		Ok(())
	}
}

/// Loads a panel's playlist
async fn load_playlist(
	playlist_player: &Mutex<PlaylistPlayer>,
	playlist_name: &PlaylistName,
	playlists: &Playlists,
) -> Result<(), AppError> {
	playlists
		.load(playlist_name.clone())
		.await
		.context("Unable to load playlist")?
		.read()
		.await
		.items
		.iter()
		.map(async |item| {
			// If not enabled, skip it
			if !item.enabled {
				return;
			}

			// Else check the kind of item
			match item.kind {
				PlaylistItemKind::Directory {
					path: ref dir_path,
					recursive,
				} =>
					WalkDir::builder()
						.max_depth(match recursive {
							true => None,
							false => Some(0),
						})
						.recurse_symlink(true)
						.build(dir_path.to_path_buf())
						.map(|entry| async {
							let entry = match entry {
								Ok(entry) => entry,
								Err(err) => {
									let err = AppError::new(&err);
									tracing::warn!("Unable to read directory entry: {}", err.pretty());
									return;
								},
							};

							let path = entry.path();
							if fs::metadata(&path)
								.await
								.map_err(|err| {
									let err = AppError::new(&err);
									tracing::warn!("Unable to get playlist entry {path:?} metadata: {}", err.pretty());
								})
								.unwrap_or_return()?
								.is_dir()
							{
								// If it's a directory, skip it
								return;
							}

							match tokio::fs::canonicalize(&path).await {
								Ok(entry) => playlist_player.lock().await.insert(entry.into()),
								Err(err) => {
									let err = AppError::new(&err);
									tracing::warn!("Unable to read playlist entry {path:?}: {}", err.pretty());
								},
							}
						})
						.collect::<FuturesUnordered<_>>()
						.await
						.collect::<()>()
						.await,

				PlaylistItemKind::File { ref path } => match tokio::fs::canonicalize(path).await {
					Ok(path) => playlist_player.lock().await.insert(path.into()),
					Err(err) => {
						let err = AppError::new(&err);
						tracing::warn!("Unable to canonicalize playlist entry {path:?}: {}", err.pretty());
					},
				},
			}
		})
		.collect::<FuturesUnordered<_>>()
		.collect::<()>()
		.await;


	Ok(())
}
