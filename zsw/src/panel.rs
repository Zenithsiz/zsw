//! Panel

// Modules
mod geometry;
mod image;
mod renderer;
mod ser;
mod state;

// Exports
pub use self::{
	geometry::PanelGeometry,
	image::{PanelImage, PanelImages},
	renderer::{PanelShader, PanelsRenderer, PanelsRendererLayouts, PanelsRendererShader},
	state::PanelState,
};

// Imports
use {
	crate::{
		image_loader::ImageRequester,
		playlist::{PlaylistItemKind, PlaylistName, PlaylistPlayer},
		shared::Shared,
		AppError,
	},
	anyhow::Context,
	futures::{stream::FuturesUnordered, StreamExt},
	std::{
		path::{Path, PathBuf},
		sync::Arc,
	},
	tokio::{fs, sync::RwLock},
	zsw_util::{Rect, UnwrapOrReturnExt, WalkDir},
	zsw_wgpu::WgpuShared,
};

/// Panels manager
#[derive(Debug)]
pub struct PanelsManager {}

impl PanelsManager {
	/// Creates a new panels manager
	pub fn new() -> Self {
		Self {}
	}

	/// Loads a panel from a path
	pub async fn load(&self, path: &Path, shared: &Arc<Shared>) -> Result<Panel, AppError> {
		// Try to read the file
		tracing::debug!(?path, "Loading panel");
		let panel_toml = tokio::fs::read_to_string(path).await.context("Unable to open file")?;

		// Then parse it
		let panel = toml::from_str::<ser::Panel>(&panel_toml).context("Unable to parse panel")?;

		// Finally convert it
		let geometries = panel.geometries.into_iter().map(|geometry| geometry.geometry).collect();
		let state = PanelState {
			paused:     false,
			progress:   0,
			duration:   panel.state.duration,
			fade_point: panel.state.fade_point,
		};
		let playlist_name = PlaylistName::from(panel.playlist);

		let panel = Panel::new(&shared.wgpu, &shared.panels_renderer_layout, geometries, state)
			.context("Unable to create panel")?;

		crate::spawn_task(format!("Load panel playlist {path:?}: {playlist_name:?}"), {
			let playlist_player = Arc::clone(&panel.playlist_player);
			let shared = Arc::clone(shared);
			|| async move {
				Self::load_playlist_into(&playlist_player, &playlist_name, &shared)
					.await
					.context("Unable to load playlist")?;

				Ok(())
			}
		});

		Ok(panel)
	}

	/// Loads `playlist` into `playlist_player`.
	// TODO: Not make `pub`?
	pub async fn load_playlist_into(
		playlist_player: &RwLock<PlaylistPlayer>,
		playlist_name: &PlaylistName,
		shared: &Shared,
	) -> Result<(), AppError> {
		/// Attempts to canonicalize `path`. If unable to, logs a warning and returns `None`
		async fn try_canonicalize_path(path: &Path) -> Option<PathBuf> {
			tokio::fs::canonicalize(path)
				.await
				.inspect_err(|err| tracing::warn!(?path, ?err, "Unable to canonicalize path"))
				.ok()
		}

		let playlist_items = {
			let playlists = shared.playlists.read().await;
			let playlist = playlists
				.get(playlist_name)
				.with_context(|| format!("Unknown playlist: {playlist_name:?}"))?;
			let playlist = playlist.read().await;
			playlist.items()
		};

		playlist_items
			.into_iter()
			.map(|item| async move {
				let item = {
					let item = item.read().await;
					item.clone()
				};

				// If not enabled, skip it
				if !item.enabled {
					tracing::trace!(?playlist_name, ?item, "Ignoring non-enabled playlist item");
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
							.map(|entry: Result<fs::DirEntry, _>| async move {
								let entry = entry
									.map_err(|err| {
										tracing::warn!(
											?playlist_name,
											?dir_path,
											?err,
											"Unable to read directory entry"
										);
									})
									.unwrap_or_return()?;

								let path = entry.path();
								if fs::metadata(&path)
									.await
									.map_err(|err| {
										tracing::warn!(?playlist_name, ?path, ?err, "Unable to get entry metadata");
									})
									.unwrap_or_return()?
									.is_dir()
								{
									// If it's a directory, skip it
									return;
								}

								let Some(path) = try_canonicalize_path(&path).await else {
									return;
								};

								let mut playlist_player = playlist_player.write().await;
								playlist_player.add(path.into());
							})
							.collect::<FuturesUnordered<_>>()
							.await
							.collect::<()>()
							.await,
					PlaylistItemKind::File { ref path } =>
						if let Some(path) = try_canonicalize_path(path).await {
							let mut playlist_player = playlist_player.write().await;
							playlist_player.add(path.into());
						},
				}
			})
			.collect::<FuturesUnordered<_>>()
			.collect::<()>()
			.await;

		// Once we're finished, clear the backlog to ensure we get proper random items after this
		// Note: If we didn't do this, the first few items would always be in the order we get them
		//       from the file system
		{
			let mut playlist_player = playlist_player.write().await;
			playlist_player.clear_backlog();
		}

		Ok(())
	}
}

/// Panel
#[derive(Debug)]
pub struct Panel {
	/// Geometries
	pub geometries: Vec<PanelGeometry>,

	/// State
	pub state: PanelState,

	/// Playlist player
	pub playlist_player: Arc<RwLock<PlaylistPlayer>>,

	/// Images
	pub images: PanelImages,
}

impl Panel {
	/// Creates a new panel
	pub fn new(
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		geometries: Vec<Rect<i32, u32>>,
		state: PanelState,
	) -> Result<Self, AppError> {
		Ok(Self {
			geometries: geometries
				.into_iter()
				.map(|geometry| PanelGeometry::new(wgpu_shared, renderer_layouts, geometry))
				.collect(),
			state,
			playlist_player: Arc::new(RwLock::new(PlaylistPlayer::new())),
			images: PanelImages::new(wgpu_shared, renderer_layouts),
		})
	}

	/// Skips to the next image
	pub async fn skip(
		&mut self,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
	) {
		self.images.step_next(wgpu_shared, renderer_layouts);
		self.state.progress = self.state.duration.saturating_sub(self.state.fade_point);

		// Then try to load the next image
		// Note: If we already have a next one, this will simply return.
		self.images
			.load_next(
				&self.playlist_player,
				wgpu_shared,
				renderer_layouts,
				image_requester,
				&self.geometries,
			)
			.await;
	}

	/// Steps this panel's state by a certain number of frames (potentially negative).
	pub async fn step(
		&mut self,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
		frames: i64,
	) {
		// Update the progress, potentially rolling over to the next image
		let next_progress = self.state.progress.saturating_add_signed(frames);
		match next_progress >= self.state.duration {
			true => {
				self.images.step_next(wgpu_shared, renderer_layouts);
				self.state.progress = next_progress.saturating_sub(self.state.fade_point);
			},
			false => {
				let max_progress = match (self.images.cur().is_loaded(), self.images.next().is_loaded()) {
					(false, false) => 0,
					(true, false) => self.state.fade_point,
					(_, true) => self.state.duration,
				};
				self.state.progress = self.state.progress.saturating_add_signed(frames).clamp(0, max_progress);
			},
		}

		// Then try to load the next image
		// Note: If we already have a next one, this will simply return.
		self.images
			.load_next(
				&self.playlist_player,
				wgpu_shared,
				renderer_layouts,
				image_requester,
				&self.geometries,
			)
			.await;
	}

	/// Updates this panel's state
	pub async fn update(
		&mut self,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
	) {
		// Then try to load the next image
		// Note: If we already have a next one, this will simply return.
		self.images
			.load_next(
				&self.playlist_player,
				wgpu_shared,
				renderer_layouts,
				image_requester,
				&self.geometries,
			)
			.await;

		// If we're paused, don't update anything
		if self.state.paused {
			return;
		}

		self.step(wgpu_shared, renderer_layouts, image_requester, 1).await;
	}
}
