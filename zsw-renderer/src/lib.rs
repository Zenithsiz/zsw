//! Renderer

// Features
#![feature(never_type)]

// Imports
use {
	anyhow::Context,
	std::time::Duration,
	tokio::time::Instant,
	winit::window::Window,
	zsw_egui::EguiRenderer,
	zsw_img::ImageReceiver,
	zsw_input::Input,
	zsw_panels::{Panels, PanelsResource},
	zsw_util::{Resources, Services},
	zsw_wgpu::{Wgpu, WgpuSurfaceResource},
};

/// Renderer
#[derive(Debug)]
#[allow(missing_copy_implementations)] // We're a service, we're not supposed to be copy
pub struct Renderer {}

impl Renderer {
	/// Creates a new renderer
	#[must_use]
	pub fn new() -> Self {
		Self {}
	}

	/// Runs the renderer
	///
	/// # Blocking
	/// Lock tree:
	/// [`zsw_panels::PanelsLock`] on `panels`
	/// [`zsw_wgpu::SurfaceLock`] on `wgpu`
	/// - [`zsw_panels::PanelsLock`] on `panels`
	pub async fn run<S, R>(&self, services: &S, resources: &R, egui_renderer: &mut EguiRenderer) -> !
	where
		S: Services<Wgpu> + Services<Window> + Services<Panels> + Services<Input> + Services<ImageReceiver>,
		R: Resources<PanelsResource> + Resources<WgpuSurfaceResource>,
	{
		// Duration we're sleeping
		let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

		loop {
			let start_time = Instant::now();

			// Update
			// DEADLOCK: Caller ensures we can lock it
			if let Err(err) = Self::update(services, resources).await {
				tracing::warn!(?err, "Unable to update");
			}

			// Render
			// DEADLOCK: Caller ensures we can lock it
			if let Err(err) = Self::render(services, resources, egui_renderer).await {
				tracing::warn!(?err, "Unable to render");
			};

			// Then sleep until next frame
			// TODO: Is it fine to measure time like this? asynchronously
			if let Some(duration) = sleep_duration.checked_sub(start_time.elapsed()) {
				tokio::time::sleep(duration).await;
			}
		}
	}

	/// Updates all panels
	///
	/// # Blocking
	/// Locks [`zsw_panels::PanelsLock`] on `panels`
	async fn update<S, R>(services: &S, resources: &R) -> Result<(), anyhow::Error>
	where
		S: Services<Wgpu> + Services<Panels> + Services<ImageReceiver>,
		R: Resources<PanelsResource>,
	{
		// DEADLOCK: Caller ensures we can lock it
		let mut panels_resource = resources.resource::<PanelsResource>().await;

		// Updates all panels
		services.service::<Panels>().update_all(
			&mut panels_resource,
			services.service::<Wgpu>(),
			services.service::<ImageReceiver>(),
		)
	}

	/// Renders
	///
	/// # Blocking
	/// Lock tree:
	/// [`zsw_wgpu::SurfaceLock`] on `wgpu`
	/// - [`zsw_panels::PanelsLock`] on `panels`
	async fn render<S, R>(services: &S, resources: &R, egui_renderer: &mut EguiRenderer) -> Result<(), anyhow::Error>
	where
		S: Services<Wgpu> + Services<Window> + Services<Panels> + Services<Input>,
		R: Resources<PanelsResource> + Resources<WgpuSurfaceResource>,
	{
		let wgpu = services.service::<Wgpu>();
		let window = services.service::<Window>();
		let panels = services.service::<Panels>();
		let input = services.service::<Input>();

		// Lock the wgpu surface
		// DEADLOCK: Caller ensures we can lock it
		let mut surface_resource = resources.resource::<WgpuSurfaceResource>().await;

		// Then render
		let surface_size = wgpu.surface_size(&surface_resource);
		let mut frame = wgpu
			.start_render(&mut surface_resource)
			.context("Unable to start render")?;

		// Render the panels
		{
			// DEADLOCK: Caller ensures we can lock it after the surface
			let panels_resource = resources.resource::<PanelsResource>().await;

			panels
				.render(
					input,
					&panels_resource,
					wgpu.queue(),
					&mut frame.encoder,
					&frame.surface_view,
					surface_size,
				)
				.context("Unable to render panels")?;
		}

		// Render egui
		// Note: If the egui painter quit, just don't render
		if let Some((egui_render_pass, egui_output, paint_jobs)) = egui_renderer.render_pass_with_output() {
			// Update textures
			#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
			let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
				physical_width:  surface_size.width,
				physical_height: surface_size.height,
				scale_factor:    window.scale_factor() as f32,
			};
			egui_render_pass
				.add_textures(wgpu.device(), wgpu.queue(), &egui_output.textures_delta)
				.context("Unable to update textures")?;

			// Update buffers
			egui_render_pass.update_buffers(wgpu.device(), wgpu.queue(), paint_jobs, &screen_descriptor);

			// Record all render passes.
			egui_render_pass
				.execute(
					&mut frame.encoder,
					&frame.surface_view,
					paint_jobs,
					&screen_descriptor,
					None,
				)
				.context("Unable to render egui")?;

			egui_render_pass
				.remove_textures(egui_output.textures_delta.clone())
				.context("Unable to update textures")?;
		}

		wgpu.finish_render(frame);

		Ok(())
	}
}

impl Default for Renderer {
	fn default() -> Self {
		Self::new()
	}
}
