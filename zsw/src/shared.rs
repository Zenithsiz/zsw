//! Shared data

// Imports
use {
	crate::{
		AppEvent,
		display::Displays,
		metrics::Metrics,
		panel::{Panels, PanelsRendererShared},
		playlist::Playlists,
		profile::Profiles,
		window::WindowMonitorNames,
	},
	std::sync::Arc,
	winit::event_loop::EventLoopProxy,
	zsw_wgpu::Wgpu,
};

/// Shared data
#[derive(Debug)]
pub struct Shared {
	pub event_loop_proxy: EventLoopProxy<AppEvent>,

	pub wgpu:                   Wgpu,
	pub panels_renderer_shared: PanelsRendererShared,

	pub displays:  Arc<Displays>,
	pub playlists: Arc<Playlists>,
	pub profiles:  Arc<Profiles>,

	pub panels: Arc<Panels>,

	pub metrics: Metrics,

	pub window_monitor_names: WindowMonitorNames,
}
