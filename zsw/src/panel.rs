//! Panel

// Modules
mod geometry;
mod image;
mod playlist_player;
mod renderer;
mod ser;
mod state;

// Exports
pub use self::{
	geometry::PanelGeometry,
	image::{ImagesState, PanelImage, PanelImages},
	playlist_player::PlaylistPlayer,
	renderer::{PanelShader, PanelsRenderer, PanelsRendererLayouts, PanelsRendererShader},
	state::{PanelParallaxState, PanelState},
};

// Imports
use {
	crate::{image_loader::ImageRequester, playlist::PlaylistItemKind, shared::Shared, AppError},
	anyhow::Context,
	async_walkdir::WalkDir,
	futures::{stream::FuturesUnordered, StreamExt},
	std::{path::Path, sync::Arc},
	tokio::sync::RwLock,
	tokio_stream::wrappers::ReadDirStream,
	zsw_util::{Rect, UnwrapOrReturnExt},
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

	/// Loads a panel group from a path
	pub async fn load(&self, path: &Path, shared: &Arc<Shared>) -> Result<PanelGroup, AppError> {
		// Try to read the file
		tracing::debug!(?path, "Loading panel group");
		let panel_group_yaml = tokio::fs::read(path).await.context("Unable to open file")?;

		// Then parse it
		let panel_group =
			serde_yaml::from_slice::<ser::PanelGroup>(&panel_group_yaml).context("Unable to parse panel group")?;

		// Finally convert it
		let panels = panel_group
			.panels
			.into_iter()
			.map(|panel| {
				let geometries = panel.geometries.into_iter().map(|geometry| geometry.geometry).collect();
				let state = PanelState {
					paused:       false,
					cur_progress: 0,
					duration:     panel.state.duration,
					fade_point:   panel.state.fade_point,
					parallax:     PanelParallaxState {
						ratio:   panel.state.parallax_ratio,
						exp:     panel.state.parallax_exp,
						reverse: panel.state.reverse_parallax,
					},
				};
				let playlist = panel.playlist;

				let panel = Panel::new(&shared.wgpu, &shared.panels_renderer_layout, geometries, state)
					.context("Unable to create panel")?;

				crate::spawn_task(format!("Load panel group {path:?}"), {
					let playlist_player = Arc::clone(&panel.playlist_player);
					let shared = Arc::clone(shared);
					|| async move {
						Self::load_playlist_into(&playlist_player, &playlist, &shared)
							.await
							.context("Unable to load playlist")?;

						Ok(())
					}
				});

				Ok::<_, AppError>(panel)
			})
			.collect::<Result<Vec<_>, _>>()
			.context("Unable to create panels")?;
		let panel_group = PanelGroup::new(panels);

		Ok(panel_group)
	}

	/// Loads `playlist` into `playlist_player`.
	// TODO: Not make `pub`?
	#[expect(clippy::too_many_lines)] // TODO: Refactor
	pub async fn load_playlist_into(
		playlist_player: &RwLock<PlaylistPlayer>,
		playlist_path: &Path,
		shared: &Shared,
	) -> Result<(), AppError> {
		/// Attempts to canonicalize `path`. If unable to, logs a warning and returns `None`
		async fn try_canonicalize_path(path: &Path) -> Option<std::path::PathBuf> {
			tokio::fs::canonicalize(path)
				.await
				.inspect_err(|err| tracing::warn!(?path, ?err, "Unable to canonicalize path"))
				.ok()
		}

		let playlist_items = {
			// DEADLOCK: We have the locker setup such that advancing from 0 to 2 cannot deadlock
			let (_, playlist) = shared
				.playlists_manager
				.load(playlist_path, &shared.playlists)
				.await
				.context("Unable to load playlist")?;
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
					tracing::trace!(?playlist_path, ?item, "Ignoring non-enabled playlist item");
					return;
				}

				// Else check the kind of item
				match item.kind {
					PlaylistItemKind::Directory { ref path, recursive } => match recursive {
						true =>
							WalkDir::new(path)
								.filter(async move |entry: async_walkdir::DirEntry| {
									match entry.file_type().await.map(|ty| ty.is_dir()) {
										Err(_) | Ok(true) => async_walkdir::Filtering::Ignore,
										Ok(false) => async_walkdir::Filtering::Continue,
									}
								})
								.map(|entry: Result<async_walkdir::DirEntry, std::io::Error>| async move {
									let entry = entry
										.map_err(|err| {
											tracing::warn!(
												?playlist_path,
												?path,
												?err,
												"Unable to read directory entry within recursive walk"
											);
										})
										.unwrap_or_return()?;

									let Some(path) = try_canonicalize_path(&entry.path()).await else {
										return;
									};

									let mut playlist_player = playlist_player.write().await;
									playlist_player.add(path.into());
								})
								.collect::<FuturesUnordered<_>>()
								.await
								.collect::<()>()
								.await,
						false => {
							let dir = tokio::fs::read_dir(path)
								.await
								.map_err(|err| {
									tracing::warn!(
										?playlist_path,
										?path,
										?err,
										"Unable to read playlist playlist directory"
									);
								})
								.unwrap_or_return()?;
							ReadDirStream::new(dir)
								.map(|entry: Result<tokio::fs::DirEntry, _>| async move {
									let entry = entry
										.map_err(|err| {
											tracing::warn!(
												?playlist_path,
												?path,
												?err,
												"Unable to read directory entry within recursive walk"
											);
										})
										.unwrap_or_return()?;

									let Some(path) = try_canonicalize_path(&entry.path()).await else {
										return;
									};

									let mut playlist_player = playlist_player.write().await;
									playlist_player.add(path.into());
								})
								.collect::<FuturesUnordered<_>>()
								.await
								.collect::<()>()
								.await;
						},
					},
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

/// Panel group
#[derive(Debug)]
pub struct PanelGroup {
	/// All panels
	panels: Vec<Panel>,
}

impl PanelGroup {
	/// Creates panels from a list of panels
	pub fn new(panels: Vec<Panel>) -> Self {
		Self { panels }
	}

	/// Returns all panels
	pub fn panels(&self) -> &[Panel] {
		&self.panels
	}

	/// Returns all panels, mutably
	pub fn panels_mut(&mut self) -> &mut Vec<Panel> {
		&mut self.panels
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

	/// Updates this panel's state
	pub async fn update(
		&mut self,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
	) {
		// If we're paused, don't update anything
		if self.state.paused {
			return;
		}

		// If we're at the end of both, swap the back image
		if self.images.state() == ImagesState::Both && self.state.cur_progress >= self.state.duration {
			self.images.swap_back(wgpu_shared, renderer_layouts);
			self.state.cur_progress = self.state.back_swapped_progress();
			return;
		}

		// Else try to load the next image
		// Note: If we have both, this will simply return.
		self.images
			.try_advance_next(
				&self.playlist_player,
				wgpu_shared,
				renderer_layouts,
				image_requester,
				&self.geometries,
			)
			.await;

		// Then update the progress, depending on the state
		self.state.cur_progress = match self.images.state() {
			ImagesState::Empty => 0,
			ImagesState::PrimaryOnly => self.state.next_progress_primary_only(),
			ImagesState::Both => self.state.next_progress_both(),
		};
	}
}
