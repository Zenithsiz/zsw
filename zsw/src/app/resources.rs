//! Resources

// Imports
use {
	futures::lock::{Mutex, MutexLockFuture},
	zsw_panels::PanelsResource,
	zsw_util::ResourcesBundle,
	zsw_wgpu::WgpuSurfaceResource,
};

/// All resources
#[derive(Debug)]
pub struct Resources {
	/// Panels
	pub panels: Mutex<PanelsResource>,

	/// Wgpu surface
	pub wgpu_surface: Mutex<WgpuSurfaceResource>,
}

impl ResourcesBundle for Resources {}

#[duplicate::duplicate_item(
	ty                 field;
	[ PanelsResource         ] [ panels ];
	[ WgpuSurfaceResource    ] [ wgpu_surface ];
)]
impl zsw_util::Resources<ty> for Resources {
	fn lock(&self) -> MutexLockFuture<ty> {
		self.field.lock()
	}
}
