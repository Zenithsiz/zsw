//! Shared data

// Imports
use {
	crate::{
		image_loader::ImageRequester,
		panel::{Panel, PanelsManager, PanelsRendererLayouts, PanelsRendererShader},
		playlist::Playlists,
		Resize,
	},
	crossbeam::atomic::AtomicCell,
	std::sync::Arc,
	tokio::sync::{Mutex, RwLock},
	winit::dpi::PhysicalPosition,
	zsw_wgpu::WgpuShared,
};

/// Shared data
#[derive(Debug)]
pub struct Shared {
	// TODO: Not have a double-`Arc` in here?
	pub window:                 Arc<winit::window::Window>,
	pub wgpu:                   WgpuShared,
	pub panels_renderer_layout: PanelsRendererLayouts,
	pub last_resize:            AtomicCell<Option<Resize>>,
	pub cursor_pos:             AtomicCell<PhysicalPosition<f64>>,

	pub panels_manager:  PanelsManager,
	pub image_requester: ImageRequester,

	pub cur_panels:             Mutex<Vec<Panel>>,
	pub panels_renderer_shader: RwLock<PanelsRendererShader>,
	pub playlists:              RwLock<Playlists>,
}
