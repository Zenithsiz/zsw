//! Shared data

// Imports
use {
	crate::{
		AppEvent,
		panel::{Panels, PanelsRendererShared},
		playlist::Playlists,
		profile::Profiles,
	},
	std::sync::{Arc, nonpoison::Mutex},
	winit::{event_loop::EventLoopProxy, window::Window},
	zsw_util::Rect,
	zsw_wgpu::Wgpu,
};

/// Shared data
#[derive(Debug)]
pub struct Shared {
	pub event_loop_proxy: EventLoopProxy<AppEvent>,

	pub wgpu:                   Wgpu,
	pub panels_renderer_shared: PanelsRendererShared,

	pub playlists: Arc<Playlists>,
	pub profiles:  Arc<Profiles>,

	pub panels: Mutex<Panels>,
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
}
