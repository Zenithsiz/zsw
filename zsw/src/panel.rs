//! Panel

// Modules
mod geometry;
mod images;
mod renderer;
mod ser;
mod state;

// Exports
pub use self::{
	geometry::PanelGeometry,
	images::{PanelImage, PanelImages},
	renderer::{PanelShader, PanelsRenderer, PanelsRendererLayouts},
	state::PanelState,
};

// Imports
use {
	crate::{
		AppError,
		image_loader::ImageRequester,
		playlist::{PlaylistItemKind, PlaylistName, PlaylistPlayer},
		shared::Shared,
	},
	core::{borrow::Borrow, fmt},
	futures::{StreamExt, stream::FuturesUnordered},
	std::{
		path::{Path, PathBuf},
		sync::Arc,
	},
	tokio::{fs, sync::RwLock},
	zsw_util::{PathAppendExt, Rect, UnwrapOrReturnExt, WalkDir},
	zsw_wgpu::WgpuShared,
	zutil_app_error::Context,
};

/// Panels manager
#[derive(Debug)]
pub struct PanelsManager {
	/// Panels directory
	root: PathBuf,
}

impl PanelsManager {
	/// Creates a new panels manager
	pub fn new(root: PathBuf) -> Self {
		Self { root }
	}

	/// Loads a panel from a name.
	///
	/// If the panel isn't for this window, returns `Ok(None)`
	pub async fn load(
		&self,
		panel_name: PanelName,
		playlist_name: PlaylistName,
		shared: &Arc<Shared>,
	) -> Result<Panel, AppError> {
		// Try to read the file
		let panel_path = self.panel_path(&panel_name);
		tracing::debug!(%panel_name, ?panel_path, "Loading panel");
		let panel_toml = tokio::fs::read_to_string(panel_path)
			.await
			.context("Unable to open file")?;

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

		let panel = Panel::new(
			panel_name.clone(),
			shared.wgpu,
			&shared.panels_renderer_layouts,
			geometries,
			state,
		)
		.context("Unable to create panel")?;

		crate::spawn_task(format!("Load panel playlist {panel_name:?}: {playlist_name:?}"), {
			let playlist_player = Arc::clone(panel.images.playlist_player());
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

	/// Returns a panel's path
	pub fn panel_path(&self, name: &PanelName) -> PathBuf {
		self.root.join(&*name.0).with_appended(".toml")
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
											%err,
											"Unable to read directory entry"
										);
									})
									.unwrap_or_return()?;

								let path = entry.path();
								if fs::metadata(&path)
									.await
									.map_err(|err| {
										tracing::warn!(?playlist_name, ?path, %err, "Unable to get entry metadata");
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

		// Once we're finished, step into the first (next) item.
		{
			let mut playlist_player = playlist_player.write().await;
			playlist_player.step_next();
		}

		Ok(())
	}
}

/// Panel
#[derive(Debug)]
pub struct Panel {
	/// Name
	pub name: PanelName,

	/// Geometries
	pub geometries: Vec<PanelGeometry>,

	/// State
	pub state: PanelState,

	/// Images
	pub images: PanelImages,
}

impl Panel {
	/// Creates a new panel
	pub fn new(
		name: PanelName,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		geometries: Vec<Rect<i32, u32>>,
		state: PanelState,
	) -> Result<Self, AppError> {
		Ok(Self {
			name,
			geometries: geometries
				.into_iter()
				.map(PanelGeometry::new)
				.collect::<Result<_, _>>()
				.context("Unable to build geometries")?,
			state,
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
		self.images.step_next(wgpu_shared, renderer_layouts).await;
		self.state.progress = self.state.duration.saturating_sub(self.state.fade_point);

		// Then load any missing images
		self.images
			.load_missing(wgpu_shared, renderer_layouts, image_requester, &self.geometries)
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
		// Update the progress, potentially rolling over to the previous/next image
		match self.state.progress.checked_add_signed(frames) {
			Some(next_progress) => match next_progress >= self.state.duration {
				true => {
					self.images.step_next(wgpu_shared, renderer_layouts).await;
					self.state.progress = next_progress.saturating_sub(self.state.duration);
				},
				false => {
					let max_progress = match (self.images.cur().is_loaded(), self.images.next().is_loaded()) {
						(false, false) => 0,
						(true, false) => self.state.fade_point,
						(_, true) => self.state.duration,
					};
					self.state.progress = self.state.progress.saturating_add_signed(frames).clamp(0, max_progress);
				},
			},
			None =>
				if self.images.step_prev(wgpu_shared, renderer_layouts).await.is_ok() {
					self.state.progress = self.state.duration.saturating_add_signed(frames);
				},
		}

		// Then load any missing images
		self.images
			.load_missing(wgpu_shared, renderer_layouts, image_requester, &self.geometries)
			.await;
	}

	/// Updates this panel's state
	pub async fn update(
		&mut self,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
	) {
		// Then load any missing images
		self.images
			.load_missing(wgpu_shared, renderer_layouts, image_requester, &self.geometries)
			.await;

		// If we're paused, don't update anything
		if self.state.paused {
			return;
		}

		self.step(wgpu_shared, renderer_layouts, image_requester, 1).await;
	}
}

/// Panel name
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Debug)]
pub struct PanelName(Arc<str>);

impl From<String> for PanelName {
	fn from(s: String) -> Self {
		Self(s.into())
	}
}

impl Borrow<str> for PanelName {
	fn borrow(&self) -> &str {
		&self.0
	}
}

impl fmt::Display for PanelName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}
