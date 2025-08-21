//! Shared data

// Imports
use {
	crate::{
		AppEvent,
		Resize,
		config_dirs::ConfigDirs,
		image_loader::ImageRequester,
		panel::{Panel, PanelShader, PanelsLoader, PanelsRendererLayouts},
		playlist::PlaylistsLoader,
	},
	crossbeam::atomic::AtomicCell,
	std::sync::Arc,
	tokio::sync::{Mutex, RwLock},
	winit::dpi::PhysicalPosition,
	zsw_util::Rect,
	zsw_wgpu::WgpuShared,
};

/// Shared data
#[derive(Debug)]
pub struct Shared {
	pub last_resize: AtomicCell<Option<Resize>>,
	pub cursor_pos:  AtomicCell<PhysicalPosition<f64>>,
	pub config_dirs: Arc<ConfigDirs>,

	pub wgpu:                    &'static WgpuShared,
	pub panels_renderer_layouts: PanelsRendererLayouts,

	pub panels_loader:    PanelsLoader,
	pub playlists_loader: PlaylistsLoader,
	pub image_requester:  ImageRequester,

	pub cur_panels:    Mutex<Vec<Panel>>,
	pub panels_shader: RwLock<PanelShader>, // TODO: Replace with atomic cell
}

/// Shared window state
#[derive(Debug)]
pub struct SharedWindow {
	// TODO: Move this to normal shared
	pub event_loop_proxy: winit::event_loop::EventLoopProxy<AppEvent>,
	pub _monitor_name:    String,
	pub monitor_geometry: Rect<i32, u32>,
	pub window:           Arc<winit::window::Window>,
}
