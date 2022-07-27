//! Resources

// Imports
use {
	futures::lock::{Mutex, MutexLockFuture},
	zsw_egui::{EguiPainterResource, EguiPlatformResource, EguiRenderPassResource},
	zsw_panels::PanelsResource,
	zsw_playlist::PlaylistResource,
	zsw_profiles::ProfilesResource,
	zsw_util::ResourcesBundle,
	zsw_wgpu::WgpuSurfaceResource,
};

/// All resources
#[derive(Debug)]
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

	/// Egui painter
	pub egui_painter: Mutex<EguiPainterResource>,
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
	[ EguiPainterResource    ] [ egui_painter ];
)]
impl zsw_util::Resources<ty> for Resources {
	fn lock(&self) -> MutexLockFuture<ty> {
		self.field.lock()
	}
}
