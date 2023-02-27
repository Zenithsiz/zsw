//! Shared data

// Modules
mod locker;
mod lockers;

// Exports
pub use self::{
	locker::AsyncMutexLocker,
	lockers::{EguiPainterLocker, LoadDefaultPanelGroupLocker, PanelsUpdaterLocker, RendererLocker},
};

// Imports
use {
	crate::{
		image_loader::ImageRequester,
		panel::{PanelsManager, PanelsRendererLayouts},
		playlist::PlaylistManager,
		wgpu_wrapper::WgpuShared,
		Resize,
	},
	crossbeam::atomic::AtomicCell,
	std::sync::Arc,
	winit::dpi::PhysicalPosition,
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

	pub playlist_manager: PlaylistManager,
	pub panels_manager:   PanelsManager,
	pub image_requester:  ImageRequester,
}
