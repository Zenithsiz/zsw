//! Shared data

// Modules
mod locker;
pub mod lockers;

// Exports
pub use self::{
	locker::{AsyncMutexLocker, AsyncRwLockLocker, LockerExt, MeetupSenderLocker},
	lockers::Locker,
};

// Imports
use {
	crate::{
		image_loader::ImageRequester,
		panel::{PanelGroup, PanelsManager, PanelsRendererLayouts, PanelsRendererShader},
		playlist::{Playlists, PlaylistsManager},
		wgpu_wrapper::WgpuShared,
		Resize,
	},
	async_lock::{Mutex, RwLock},
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

	pub panels_manager:    PanelsManager,
	pub image_requester:   ImageRequester,
	pub playlists_manager: PlaylistsManager,

	pub cur_panel_group:        Mutex<Option<PanelGroup>>,
	pub panels_renderer_shader: RwLock<PanelsRendererShader>,
	pub playlists:              RwLock<Playlists>,
}
