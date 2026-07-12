//! Egui wrapper

// Features
#![feature(must_not_suspend, nonpoison_mutex, sync_nonpoison)]

// Imports
use {
	egui::epaint,
	std::{
		fmt,
		sync::{Arc, nonpoison::Mutex},
	},
	tracing as _,
	winit::{event::WindowEvent, window::Window},
	zsw_util::AppError,
	zsw_wgpu::{FrameRender, Wgpu, WgpuRenderer},
};

/// Egui Renderer
pub struct EguiRenderer {
	/// Renderer
	renderer: egui_wgpu::Renderer,
}

impl fmt::Debug for EguiRenderer {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("EguiRenderer").field("render_pass", &"..").finish()
	}
}

impl EguiRenderer {
	/// Creates a new egui renderer
	#[must_use]
	pub fn new(wgpu_renderer: &WgpuRenderer, wgpu: &Wgpu) -> Self {
		// Create the egui renderer
		let renderer = egui_wgpu::Renderer::new(
			&wgpu.device,
			wgpu_renderer.surface_config().format,
			egui_wgpu::RendererOptions::default(),
		);

		Self { renderer }
	}

	/// Renders egui
	pub fn render_egui(
		&mut self,
		frame: &mut FrameRender,
		window: &Window,
		wgpu: &Wgpu,
		paint_jobs: &[egui::ClippedPrimitive],
		textures_delta: Option<&egui::TexturesDelta>,
	) -> Result<(), AppError> {
		// Update textures
		#[expect(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
		let screen_descriptor = egui_wgpu::ScreenDescriptor {
			size_in_pixels:   [frame.surface_size.width, frame.surface_size.height],
			pixels_per_point: window.scale_factor() as f32,
		};

		// If we have any textures delta, update them
		if let Some(textures_delta) = textures_delta.as_ref() {
			for &(id, ref delta) in &textures_delta.set {
				self.renderer.update_texture(&wgpu.device, &wgpu.queue, id, delta);
			}
			for id in &textures_delta.free {
				self.renderer.free_texture(id);
			}
		}

		// Update buffers
		let buffers = self.renderer.update_buffers(
			&wgpu.device,
			&wgpu.queue,
			&mut frame.encoder,
			paint_jobs,
			&screen_descriptor,
		);
		let _: wgpu::SubmissionIndex = wgpu.queue.submit(buffers);

		// Record all render passes.
		let render_pass_color_attachment = wgpu::RenderPassColorAttachment {
			view:           &frame.surface_view,
			depth_slice:    None,
			resolve_target: None,
			ops:            wgpu::Operations {
				load:  wgpu::LoadOp::Load,
				store: wgpu::StoreOp::Store,
			},
		};
		let render_pass_descriptor = wgpu::RenderPassDescriptor {
			label:                    Some("zsw-egui-render-pass"),
			color_attachments:        &[Some(render_pass_color_attachment)],
			depth_stencil_attachment: None,
			timestamp_writes:         None,
			occlusion_query_set:      None,
			multiview_mask:           None,
		};
		let render_pass = frame.encoder.begin_render_pass(&render_pass_descriptor);
		let mut render_pass = render_pass.forget_lifetime();
		self.renderer.render(&mut render_pass, paint_jobs, &screen_descriptor);

		Ok(())
	}
}

/// Egui drawer
pub struct EguiPainter {
	/// Window
	window: Arc<Window>,

	/// Context
	ctx: egui::Context,

	/// State
	state: Arc<Mutex<egui_winit::State>>,
}

impl fmt::Debug for EguiPainter {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("EguiPainter").field("platform", &"..").finish()
	}
}

impl EguiPainter {
	#[must_use]
	pub fn new(event_handler: &EguiEventHandler, ctx: egui::Context) -> Self {
		Self {
			window: Arc::clone(&event_handler.window),
			ctx,
			state: Arc::clone(&event_handler.state),
		}
	}

	/// Draws egui
	pub fn draw<E>(&self, mut f: impl FnMut(&egui::Context) -> Result<(), E>) -> Result<egui::FullOutput, E> {
		let input = self.state.lock().take_egui_input(&self.window);

		let mut res = Ok(());
		let full_output = self.ctx.run_ui(input, |ctx| {
			if let Err(err) = f(ctx) {
				res = Err(err);
			}
		});
		res?;

		Ok(full_output)
	}

	/// Tessellate the output shapes
	#[must_use]
	pub fn tessellate_shapes(
		&self,
		shapes: Vec<epaint::ClippedShape>,
		pixels_per_point: f32,
	) -> Vec<egui::ClippedPrimitive> {
		self.ctx.tessellate(shapes, pixels_per_point)
	}
}

/// Egui Event handler
pub struct EguiEventHandler {
	/// Window
	window: Arc<Window>,

	/// State
	state: Arc<Mutex<egui_winit::State>>,
}

impl fmt::Debug for EguiEventHandler {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("EguiEventHandler").field("platform", &"..").finish()
	}
}

impl EguiEventHandler {
	#[must_use]
	pub fn new(wgpu: &Wgpu, window: Arc<Window>, ctx: egui::Context) -> Self {
		// Create the egui platform
		let viewport_id = egui::ViewportId::from_hash_of(window.id());

		let state = egui_winit::State::new(
			ctx,
			viewport_id,
			&window,
			None,
			None,
			Some(wgpu.device.limits().max_texture_dimension_2d as usize),
		);
		let state = Arc::new(Mutex::new(state));

		Self { window, state }
	}

	/// Handles an event.
	///
	/// Returns if egui wants exclusive use of the event
	#[must_use]
	pub fn handle_event(&self, event: &WindowEvent) -> bool {
		let response = self.state.lock().on_window_event(&self.window, event);
		response.consumed
	}
}
