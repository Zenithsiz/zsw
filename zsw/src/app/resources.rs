//! Resources

// Imports
use {
	futures::lock::{Mutex, MutexLockFuture},
	zsw_egui::{EguiPlatformResource, EguiRenderPassResource},
	zsw_panels::PanelsResource,
	zsw_playlist::PlaylistResource,
	zsw_profiles::ProfilesResource,
	zsw_util::{ResourcesBundle, ResourcesLock},
	zsw_wgpu::WgpuSurfaceResource,
};

/// All resources
pub struct Resources {
	/// Panels
	pub panels: Mutex<PanelsResource>,

	/// Playlist
	pub playlist: Mutex<PlaylistResource>,

	/// Profiles
	pub profiles: Mutex<ProfilesResource>,

	/// Wgpu surface
	pub wgpu_surface: Mutex<WgpuSurfaceResource>,

	/// Egui platform
	pub egui_platform: Mutex<EguiPlatformResource>,

	/// Egui render pass
	pub egui_render_pass: Mutex<EguiRenderPassResource>,
}

impl ResourcesBundle for Resources {}

#[duplicate::duplicate_item(
	ty                 field;
	[ PanelsResource         ] [ panels ];
	[ PlaylistResource       ] [ playlist ];
	[ ProfilesResource       ] [ profiles ];
	[ WgpuSurfaceResource    ] [ wgpu_surface ];
	[ EguiPlatformResource   ] [ egui_platform ];
	[ EguiRenderPassResource ] [ egui_render_pass ];
)]
impl ResourcesLock<ty> for Resources {
	fn lock(&self) -> MutexLockFuture<ty> {
		self.field.lock()
	}
}
