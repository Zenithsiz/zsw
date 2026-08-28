//! Egui wrapper

// Features
#![feature(must_not_suspend)]

// Imports
use {
	std::sync::Arc,
	tracing as _,
	winit::{event::WindowEvent, window::Window},
	zsw_wgpu::{FrameRender, Wgpu, WgpuRenderer},
};

/// Egui
#[derive(derive_more::Debug)]
pub struct Egui {
	/// Window
	window: Arc<Window>,

	/// Context
	ctx: egui::Context,

	/// State
	#[debug("..")]
	state: egui_winit::State,

	/// Renderer
	#[debug("..")]
	renderer: egui_wgpu::Renderer,
}

impl Egui {
	/// Creates a new egui
	#[must_use]
	pub fn new(wgpu: &Wgpu, wgpu_renderer: &WgpuRenderer, window: Arc<Window>) -> Self {
		let renderer = egui_wgpu::Renderer::new(
			&wgpu.device,
			wgpu_renderer.surface_config().format,
			egui_wgpu::RendererOptions::default(),
		);

		let viewport_id = egui::ViewportId::from_hash_of(window.id());
		let ctx = egui::Context::default();
		let state = egui_winit::State::new(
			ctx.clone(),
			viewport_id,
			&window,
			None,
			None,
			Some(wgpu.device.limits().max_texture_dimension_2d as usize),
		);

		Self {
			window,
			ctx,
			state,
			renderer,
		}
	}

	/// Renders egui
	pub fn render(&mut self, frame: &mut FrameRender, window: &Window, wgpu: &Wgpu, draw: impl FnMut(&mut egui::Ui)) {
		// Paint
		let input = self.state.take_egui_input(&self.window);
		let mut full_output = self.ctx.run_ui(input, draw);
		let paint_jobs = self.ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

		// Update textures
		#[expect(clippy::iter_over_hash_type, reason = "We receive it like that")]
		for (&id, deltas) in &full_output.textures_delta.set {
			for delta in deltas {
				self.renderer.update_texture(&wgpu.device, &wgpu.queue, id, delta);
			}
		}
		#[expect(clippy::iter_over_hash_type, reason = "We receive it like that")]
		for id in &full_output.textures_delta.free {
			self.renderer.free_texture(id);
		}
		full_output.textures_delta.clear();

		// Update buffers
		#[expect(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
		let screen_descriptor = egui_wgpu::ScreenDescriptor {
			size_in_pixels:   [frame.surface_size.width, frame.surface_size.height],
			pixels_per_point: window.scale_factor() as f32,
		};
		let buffers = self.renderer.update_buffers(
			&wgpu.device,
			&wgpu.queue,
			&mut frame.encoder,
			&paint_jobs,
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
		self.renderer.render(&mut render_pass, &paint_jobs, &screen_descriptor);
	}

	/// Handles an event.
	///
	/// Returns if egui wants exclusive use of the event
	#[must_use]
	pub fn handle_event(&mut self, event: &WindowEvent) -> bool {
		let response = self.state.on_window_event(&self.window, event);
		response.consumed
	}
}
