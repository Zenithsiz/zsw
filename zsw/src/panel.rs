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
	crate::{
		image_loader::ImageRequester,
		playlist::PlaylistItemKind,
		shared::{AsyncLocker, AsyncRwLockResource, LockerIteratorExt, LockerStreamExt, PlaylistPlayerRwLock, Shared},
		AppError,
	},
	anyhow::Context,
	async_walkdir::WalkDir,
	futures::TryStreamExt,
	std::{path::Path, sync::Arc},
	tokio_stream::wrappers::ReadDirStream,
	zsw_util::Rect,
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

				#[allow(clippy::let_underscore_future)] // It's a spawned future and we don't care about joining
				let _ = tokio::spawn({
					let playlist_player = Arc::clone(&panel.playlist_player);
					let shared = Arc::clone(shared);
					async move {
						// DEADLOCK: This is a new task
						let mut locker = AsyncLocker::new();
						match Self::load_playlist_into(&playlist_player, &playlist, &shared, &mut locker).await {
							Ok(()) => tracing::debug!(?playlist, "Loaded playlist"),
							Err(err) => tracing::debug!(?playlist, ?err, "Unable to load playlist"),
						}
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
	async fn load_playlist_into(
		playlist_player: &PlaylistPlayerRwLock,
		playlist: &Path,
		shared: &Shared,
		locker: &mut AsyncLocker<'_, 0>,
	) -> Result<(), AppError> {
		let playlist_items = {
			let playlist = shared
				.playlists_manager
				.load(playlist, &shared.playlists, locker)
				.await
				.context("Unable to load playlist")?;
			let (playlist, _) = playlist.read(locker).await;
			playlist.items()
		};

		playlist_items
			.into_iter()
			.split_locker_async_unordered(locker, |item, mut locker| async move {
				let item = {
					let (item, _) = item.read(&mut locker).await;
					item.clone()
				};

				// If not enabled, skip it
				if !item.enabled {
					tracing::trace!(?item, "Ignoring non-enabled playlist item");
					return Ok(());
				}

				// Else check the kind of item
				match item.kind {
					PlaylistItemKind::Directory { ref path, recursive } => match recursive {
						true => WalkDir::new(path)
							.filter(async move |entry| match entry.file_type().await.map(|ty| ty.is_dir()) {
								Err(_) | Ok(true) => async_walkdir::Filtering::Ignore,
								Ok(false) => async_walkdir::Filtering::Continue,
							})
							.map_err(anyhow::Error::new)
							.split_locker_async_unordered(&mut locker, async move |entry, mut locker| {
								let path = tokio::fs::canonicalize(entry?.path())
									.await
									.context("Unable to canonicalize path")?;

								let (mut playlist_player, _) = playlist_player.write(&mut locker).await;
								playlist_player.add(path.into());

								Ok::<_, AppError>(())
							})
							.try_collect()
							.await
							.context("Unable to recursively read directory files")?,
						false => {
							let dir = tokio::fs::read_dir(path).await.context("Unable to read directory")?;
							ReadDirStream::new(dir)
								.map_err(anyhow::Error::new)
								.split_locker_async_unordered(&mut locker, async move |entry, mut locker| {
									let path = tokio::fs::canonicalize(entry?.path())
										.await
										.context("Unable to canonicalize path")?;

									let (mut playlist_player, _) = playlist_player.write(&mut locker).await;
									playlist_player.add(path.into());

									Ok::<_, AppError>(())
								})
								.try_collect()
								.await
								.context("Unable to read directory files")?;
						},
					},
					PlaylistItemKind::File { ref path } => {
						let path = tokio::fs::canonicalize(path)
							.await
							.context("Unable to canonicalize path")?;

						let (mut playlist_player, _) = playlist_player.write(&mut locker).await;
						playlist_player.add(path.into());
					},
				}

				Ok::<_, AppError>(())
			})
			.try_collect::<()>()
			.await
			.context("Unable to load all items")?;

		// Once we're finished, clear the backlog to ensure we get proper random items after this
		// Note: If we didn't do this, the first few items would always be in the order we get them
		//       from the file system
		{
			let (mut playlist_player, _) = playlist_player.write(locker).await;
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
	pub playlist_player: Arc<PlaylistPlayerRwLock>,

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
			playlist_player: Arc::new(PlaylistPlayerRwLock::new(PlaylistPlayer::new())),
			images: PanelImages::new(wgpu_shared, renderer_layouts),
		})
	}

	/// Updates this panel's state
	pub async fn update(
		&mut self,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
		locker: &mut AsyncLocker<'_, 1>,
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
				locker,
			)
			.await;

		// Then update the progress, depending on the state
		self.state.cur_progress = match self.images.state() {
			// If empty, or primary only,
			ImagesState::Empty => 0,
			ImagesState::PrimaryOnly => self.state.next_progress_primary_only(),
			ImagesState::Both => self.state.next_progress_both(),
		};
	}
}
