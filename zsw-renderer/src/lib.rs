//! Renderer

// Features
#![feature(never_type)]

// Imports
use {
	anyhow::Context,
	std::{mem, time::Duration},
	tokio::time::Instant,
	winit::window::Window,
	zsw_egui::{Egui, EguiPaintJobsResource, EguiPlatformResource, EguiRenderPassResource},
	zsw_img::ImageLoader,
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
	/// - [`zsw_egui::RenderPassLock`] on `egui`
	///   - [`zsw_egui::PlatformLock`] on `egui`
	pub async fn run<S, R>(&self, services: &S, resources: &R) -> !
	where
		S: Services<Wgpu>
			+ Services<Egui>
			+ Services<Window>
			+ Services<Panels>
			+ Services<Input>
			+ Services<ImageLoader>,
		R: Resources<PanelsResource>
			+ Resources<WgpuSurfaceResource>
			+ Resources<EguiPlatformResource>
			+ Resources<EguiRenderPassResource>
			+ Resources<EguiPaintJobsResource>,
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
			if let Err(err) = Self::render(services, resources).await {
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
		S: Services<Wgpu> + Services<Panels> + Services<ImageLoader>,
		R: Resources<PanelsResource>,
	{
		// DEADLOCK: Caller ensures we can lock it
		let mut panels_resource = resources.resource::<PanelsResource>().await;

		// Updates all panels
		services.service::<Panels>().update_all(
			&mut panels_resource,
			services.service::<Wgpu>(),
			services.service::<ImageLoader>(),
		)
	}

	/// Renders
	///
	/// # Blocking
	/// Lock tree:
	/// [`zsw_wgpu::SurfaceLock`] on `wgpu`
	/// - [`zsw_panels::PanelsLock`] on `panels`
	/// - [`zsw_egui::PaintJobsLock`] on `egui`
	///   - [`zsw_egui::RenderPassLock`] on `egui`
	///     - [`zsw_egui::PlatformLock`] on `egui`
	async fn render<S, R>(services: &S, resources: &R) -> Result<(), anyhow::Error>
	where
		S: Services<Wgpu> + Services<Egui> + Services<Window> + Services<Panels> + Services<Input>,
		R: Resources<PanelsResource>
			+ Resources<WgpuSurfaceResource>
			+ Resources<EguiPlatformResource>
			+ Resources<EguiRenderPassResource>
			+ Resources<EguiPaintJobsResource>,
	{
		let wgpu = services.service::<Wgpu>();
		let egui = services.service::<Egui>();
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

		#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
		let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
			physical_width:  surface_size.width,
			physical_height: surface_size.height,
			scale_factor:    window.scale_factor() as f32,
		};

		// Get the egui render results
		// DEADLOCK: Caller ensures we can lock it.
		let mut egui_paint_jobs_resource = resources.resource::<EguiPaintJobsResource>().await;
		let egui_paint_jobs = egui.paint_jobs(&mut egui_paint_jobs_resource);

		// If we have any paint jobs, draw egui
		if !egui_paint_jobs.is_empty() {
			// DEADLOCK: Caller ensures we can lock it after the wgpu surface lock
			let mut render_pass_resource = resources.resource::<EguiRenderPassResource>().await;
			let egui_render_pass = egui.render_pass(&mut render_pass_resource);

			let font_image = {
				// DEADLOCK: Caller ensures we can lock it after the egui render pass lock
				let platform_resource = resources.resource::<EguiPlatformResource>().await;
				egui.font_image(&platform_resource)
			};

			egui_render_pass.update_texture(wgpu.device(), wgpu.queue(), &font_image);
			egui_render_pass.update_user_textures(wgpu.device(), wgpu.queue());
			egui_render_pass.update_buffers(wgpu.device(), wgpu.queue(), egui_paint_jobs, &screen_descriptor);

			// Record all render passes.
			egui_render_pass
				.execute(
					&mut frame.encoder,
					&frame.surface_view,
					egui_paint_jobs,
					&screen_descriptor,
					None,
				)
				.context("Unable to render egui")?;
		}

		mem::drop(egui_paint_jobs_resource);
		wgpu.finish_render(frame);

		Ok(())
	}
}

impl Default for Renderer {
	fn default() -> Self {
		Self::new()
	}
}
