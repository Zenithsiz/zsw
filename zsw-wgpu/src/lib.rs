//! Wgpu wrapper

#![feature(must_not_suspend, yeet_expr, share_trait)]

mod renderer;

pub use renderer::{FrameRender, WgpuRenderer};

use {app_error::Context, winit::event_loop::OwnedDisplayHandle, zsw_util::AppError};

/// Wgpu
#[derive(Debug)]
pub struct Wgpu {
	/// Instance
	pub instance: wgpu::Instance,
}

impl Wgpu {
	/// Creates the wgpu.
	///
	pub fn new(display: OwnedDisplayHandle, force_opengl: bool) -> Result<Self, AppError> {
		let instance = self::create_instance(display, force_opengl).context("Unable to create instance")?;

		Ok(Self { instance })
	}
}

/// Creates the instance
fn create_instance(display: OwnedDisplayHandle, force_opengl: bool) -> Result<wgpu::Instance, AppError> {
	let mut instance_desc = wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display));
	if force_opengl {
		instance_desc.backends = wgpu::Backends::GL;
	}
	tracing::debug!(?instance_desc, "Requesting wgpu instance");
	let instance = wgpu::Instance::new(instance_desc);
	tracing::debug!(?instance, "Created wgpu instance");

	Ok(instance)
}
