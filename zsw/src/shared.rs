//! Shared data

// Imports
use {
	crate::{
		AppEvent,
		Resize,
		config_dirs::ConfigDirs,
		image_loader::ImageRequester,
		panel::{Panel, PanelShader, PanelsManager, PanelsRendererLayouts},
		playlist::Playlists,
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
	pub event_loop_proxy:       winit::event_loop::EventLoopProxy<AppEvent>,
	pub window:                 &'static winit::window::Window,
	pub wgpu:                   WgpuShared,
	pub panels_renderer_layout: PanelsRendererLayouts,
	pub last_resize:            AtomicCell<Option<Resize>>,
	pub cursor_pos:             AtomicCell<PhysicalPosition<f64>>,
	pub config_dirs:            Arc<ConfigDirs>,

	pub panels_manager:  PanelsManager,
	pub image_requester: ImageRequester,

	pub cur_panels:    Mutex<Vec<Panel>>,
	pub panels_shader: RwLock<PanelShader>, // TODO: Replace with atomic cell
	pub playlists:     RwLock<Playlists>,
}
