//! Shared data

// Imports
use {
	crate::{
		AppEvent,
		config::Config,
		panel::{Panels, PanelsRendererShared},
		playlist::Playlists,
		profile::Profiles,
	},
	std::sync::Arc,
	winit::{event_loop::EventLoopProxy, window::Window},
	zsw_util::Rect,
	zsw_wgpu::Wgpu,
};

/// Shared data
#[derive(Debug)]
pub struct Shared {
	pub event_loop_proxy: EventLoopProxy<AppEvent>,

	pub config: Config,

	pub wgpu:                   Wgpu,
	pub panels_renderer_shared: PanelsRendererShared,

	pub playlists: Arc<Playlists>,
	pub profiles:  Arc<Profiles>,
}

/// Shared window data
#[derive(Debug)]
pub struct SharedWindow {
	/// Window
	pub window: Arc<Window>,

	/// Monitor name
	pub monitor_name: String,

	/// Monitor geometry
	pub monitor_geometry: Rect<i32, u32>,

	/// Monitor refresh rate (in mHz)
	pub monitor_refresh_rate_mhz: u32,

	/// Panels
	pub panels: Panels,
}
