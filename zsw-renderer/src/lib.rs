//! Renderer

// Features
#![feature(never_type)]

// Imports
use {
	anyhow::Context,
	cgmath::Point2,
	std::{mem, sync::Arc, time::Duration},
	tokio::time::Instant,
	winit::window::Window,
	zsw_egui::EguiRenderer,
	zsw_img::{ImageReceiver, RawImageProvider},
	zsw_input::InputReceiver,
	zsw_panels::{PanelsEditor, PanelsRenderer, PanelsResource},
	zsw_util::{Resources, ResourcesTuple2},
	zsw_wgpu::{Wgpu, WgpuRenderer, WgpuSurfaceResource},
};

/// Renderer
#[derive(Debug)]
#[allow(missing_copy_implementations)] // We're a service, we're not supposed to be copy
pub struct Renderer<P: RawImageProvider> {
	/// Panels renderer
	panels_renderer: PanelsRenderer,

	/// Egui renderer
	egui_renderer: EguiRenderer,

	/// Input receiver
	input_receiver: InputReceiver,

	/// Wgpu renderer
	wgpu_renderer: WgpuRenderer,

	/// Wgpu
	wgpu: Wgpu,

	/// Window
	window: Arc<Window>,

	/// Image receiver
	image_receiver: ImageReceiver<P>,

	/// Panels editor
	panels_editor: PanelsEditor,
}

impl<P: RawImageProvider> Renderer<P> {
	/// Creates a new renderer
	#[must_use]
	#[allow(clippy::too_many_arguments)] // We can't do much about it, it's a constructor
	pub fn new(
		panels_renderer: PanelsRenderer,
		egui_renderer: EguiRenderer,
		input_receiver: InputReceiver,
		wgpu_renderer: WgpuRenderer,
		wgpu: Wgpu,
		window: Arc<Window>,
		image_receiver: ImageReceiver<P>,
		panels_editor: PanelsEditor,
	) -> Self {
		Self {
			panels_renderer,
			egui_renderer,
			input_receiver,
			wgpu_renderer,
			wgpu,
			window,
			image_receiver,
			panels_editor,
		}
	}

	/// Runs the renderer
	pub async fn run<R>(mut self, resources: &mut R) -> !
	where
		R: Resources<PanelsResource> + ResourcesTuple2<PanelsResource, WgpuSurfaceResource>,
	{
		// Duration we're sleeping
		let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

		loop {
			let start_time = Instant::now();

			// Update
			if let Err(err) = self.update(resources).await {
				tracing::warn!(?err, "Unable to update");
			}

			// Render
			if let Err(err) = self.render(resources).await {
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
	async fn update<R>(&mut self, resources: &mut R) -> Result<(), anyhow::Error>
	where
		R: Resources<PanelsResource>,
	{
		let mut panels_resource = resources.resource::<PanelsResource>().await;

		// Updates all panels
		let max_image_size = self.panels_editor.max_image_size(&panels_resource);
		self.panels_renderer
			.update_all(&mut panels_resource, &self.wgpu, &self.image_receiver, max_image_size)
	}

	/// Renders
	async fn render<R>(&mut self, resources: &mut R) -> Result<(), anyhow::Error>
	where
		R: ResourcesTuple2<PanelsResource, WgpuSurfaceResource>,
	{
		let (panels_resource, mut surface_resource) = resources.resources_tuple2().await;

		// Then render
		let surface_size = self.wgpu.surface_size(&surface_resource);
		let mut frame = self
			.wgpu_renderer
			.start_render(&mut surface_resource)
			.context("Unable to start render")?;

		// Render the panels
		{
			let cursor_pos = self
				.input_receiver
				.cursor_pos()
				.map_or(Point2::new(0, 0), |pos| Point2::new(pos.x as i32, pos.y as i32));

			self.panels_renderer
				.render(
					&panels_resource,
					cursor_pos,
					self.wgpu.queue(),
					&mut frame.encoder,
					&frame.surface_view,
					surface_size,
				)
				.context("Unable to render panels")?;
			mem::drop(panels_resource);
		}

		// Render egui
		// Note: If the egui painter quit, just don't render
		if let Some((egui_render_pass, egui_output, paint_jobs)) = self.egui_renderer.render_pass_with_output() {
			// Update textures
			#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
			let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
				physical_width:  surface_size.width,
				physical_height: surface_size.height,
				scale_factor:    self.window.scale_factor() as f32,
			};
			egui_render_pass
				.add_textures(self.wgpu.device(), self.wgpu.queue(), &egui_output.textures_delta)
				.context("Unable to update textures")?;

			// Update buffers
			egui_render_pass.update_buffers(self.wgpu.device(), self.wgpu.queue(), paint_jobs, &screen_descriptor);

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

		self.wgpu_renderer.finish_render(frame, &mut surface_resource);
		mem::drop(surface_resource);

		Ok(())
	}
}
