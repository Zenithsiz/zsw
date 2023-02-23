//! Panel

// Modules
mod geometry;
mod image;
mod playlist_player;
mod renderer;
mod ser_de;
mod state;

// Exports
pub use self::{
	geometry::PanelGeometry,
	image::{ImagesState, PanelImage, PanelImages},
	playlist_player::PlaylistPlayer,
	renderer::{PanelShader, PanelsRenderer, PanelsRendererLayouts, PanelsRendererShader},
	state::PanelState,
};

// Imports
use {
	self::ser_de::SerPanelGroup,
	crate::{
		image_loader::ImageRequester,
		playlist::{Playlist, PlaylistManager},
		wgpu_wrapper::WgpuShared,
	},
	anyhow::Context,
	futures::{stream::FuturesUnordered, TryStreamExt},
	std::path::PathBuf,
	zsw_util::{PathAppendExt, Rect},
};

/// Panels manager
#[derive(Debug)]
pub struct PanelsManager {
	/// Base Directory
	base_dir: PathBuf,
}

impl PanelsManager {
	/// Creates a new panels manager
	pub fn new(base_dir: PathBuf) -> Self {
		Self { base_dir }
	}

	/// Loads a panel group from disk
	pub async fn load(
		&self,
		name: &str,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		playlist_manager: &PlaylistManager,
	) -> Result<PanelGroup, anyhow::Error> {
		// Try to read the file
		let path = self.base_dir.join(name).with_appended(".yaml");
		tracing::debug!(?name, ?path, "Loading panel group");
		let panel_group_yaml = tokio::fs::read(path).await.context("Unable to open file")?;

		// Then parse it
		let panel_group =
			serde_yaml::from_slice::<SerPanelGroup>(&panel_group_yaml).context("Unable to parse panel group")?;

		// Finally convert it
		let panels = panel_group
			.panels
			.into_iter()
			.map(async move |panel| {
				let geometries = panel.geometries.into_iter().map(|geometry| geometry.geometry).collect();
				let state = PanelState {
					cur_progress:     0,
					duration:         panel.state.duration,
					fade_point:       panel.state.fade_point,
					parallax_ratio:   panel.state.parallax_ratio,
					parallax_exp:     panel.state.parallax_exp,
					reverse_parallax: panel.state.reverse_parallax,
				};
				let playlist = playlist_manager
					.get(&panel.playlist)
					.await
					.context("Unable to load playlist")?;

				Panel::new(wgpu_shared, renderer_layouts, geometries, state, &playlist)
					.await
					.context("Unable to create panel")
			})
			.collect::<FuturesUnordered<_>>()
			.try_collect()
			.await
			.context("Unable to create panels")?;
		let panel_group = PanelGroup::new(panels);

		Ok(panel_group)
	}
}

/// Panel group
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
	pub playlist_player: PlaylistPlayer,

	/// Images
	pub images: PanelImages,
}

impl Panel {
	/// Creates a new panel
	pub async fn new(
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		geometries: Vec<Rect<i32, u32>>,
		state: PanelState,
		playlist: &Playlist,
	) -> Result<Self, anyhow::Error> {
		Ok(Self {
			geometries: geometries
				.into_iter()
				.map(|geometry| PanelGeometry::new(wgpu_shared, renderer_layouts, geometry))
				.collect(),
			state,
			playlist_player: PlaylistPlayer::new(playlist)
				.await
				.context("Unable to create playlist player")?,
			images: PanelImages::new(wgpu_shared, renderer_layouts),
		})
	}

	/// Updates this panel's state
	pub fn update(
		&mut self,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
	) {
		// If we're at the end of both, swap the back image
		if self.images.state() == ImagesState::Both && self.state.cur_progress >= self.state.duration {
			self.images.swap_back(wgpu_shared, renderer_layouts);
			self.state.cur_progress = self.state.back_swapped_progress();
			return;
		}

		// Else try to load the next image
		// Note: If we have both, this will simply return.
		self.images.try_advance_next(
			&mut self.playlist_player,
			wgpu_shared,
			renderer_layouts,
			image_requester,
			&self.geometries,
		);

		// Then update the progress, depending on the state
		self.state.cur_progress = match self.images.state() {
			// If empty, or primary only,
			ImagesState::Empty => 0,
			ImagesState::PrimaryOnly => self.state.next_progress_primary_only(),
			ImagesState::Both => self.state.next_progress_both(),
		};
	}
}