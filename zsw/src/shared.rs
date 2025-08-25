//! Shared data

// Imports
use {
	crate::{
		AppEvent,
		Resize,
		config_dirs::ConfigDirs,
		panel::{PanelImages, PanelName, Panels, PanelsGeometryUniforms, PanelsRendererLayouts},
		playlist::Playlists,
	},
	crossbeam::atomic::AtomicCell,
	std::{collections::HashMap, sync::Arc},
	tokio::sync::Mutex,
	winit::dpi::PhysicalPosition,
	zsw_util::Rect,
	zsw_wgpu::WgpuShared,
};

/// Shared data
#[derive(Debug)]
pub struct Shared {
	pub event_loop_proxy: winit::event_loop::EventLoopProxy<AppEvent>,

	pub last_resize: AtomicCell<Option<Resize>>,
	pub cursor_pos:  AtomicCell<PhysicalPosition<f64>>,

	pub config_dirs: Arc<ConfigDirs>,

	pub wgpu:                    &'static WgpuShared,
	pub panels_renderer_layouts: PanelsRendererLayouts,

	pub panels:    Panels,
	pub playlists: Playlists,

	pub panels_images: Mutex<HashMap<PanelName, PanelImages>>,
}

/// Shared window state
#[derive(Debug)]
pub struct SharedWindow {
	pub _monitor_name:            String,
	pub monitor_geometry:         Rect<i32, u32>,
	pub window:                   Arc<winit::window::Window>,
	pub panels_geometry_uniforms: Mutex<PanelsGeometryUniforms>,
}
