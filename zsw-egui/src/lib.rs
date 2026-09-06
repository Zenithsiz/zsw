//! Egui wrapper

#![feature(must_not_suspend)]

mod wayland;

pub use self::wayland::EguiWaylandState;

use {
	tracing as _,
	zsw_wayland::WaylandData,
	zsw_wgpu::{FrameRender, WgpuRenderer},
};

/// Egui
#[derive(derive_more::Debug)]
pub struct Egui {
	/// Context
	ctx: egui::Context,

	/// Renderer
	#[debug("..")]
	renderer: egui_wgpu::Renderer,
}

impl Egui {
	/// Creates a new egui
	#[must_use]
	pub fn new(wgpu_renderer: &WgpuRenderer) -> Self {
		let renderer = egui_wgpu::Renderer::new(
			&wgpu_renderer.device,
			wgpu_renderer.surface_config.format,
			egui_wgpu::RendererOptions::default(),
		);

		let ctx = egui::Context::default();

		Self { ctx, renderer }
	}

	/// Paints egui
	pub fn paint(&mut self, input: egui::RawInput, draw: impl FnMut(&mut egui::Ui)) -> egui::FullOutput {
		self.ctx.run_ui(input, draw)
	}

	/// Renders egui after painting
	pub fn render<A>(
		&mut self,
		frame: &mut FrameRender,
		wayland_data: &mut WaylandData<A>,
		wgpu_renderer: &WgpuRenderer,
		mut full_output: egui::FullOutput,
	) -> egui::PlatformOutput {
		let paint_jobs = self.ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

		// Update textures
		#[expect(clippy::iter_over_hash_type, reason = "We receive it like that")]
		for (&id, deltas) in &full_output.textures_delta.set {
			for delta in deltas {
				self.renderer
					.update_texture(&wgpu_renderer.device, &wgpu_renderer.queue, id, delta);
			}
		}
		#[expect(clippy::iter_over_hash_type, reason = "We receive it like that")]
		for id in &full_output.textures_delta.free {
			self.renderer.free_texture(id);
		}
		full_output.textures_delta.clear();

		// Update buffers
		let screen_descriptor = egui_wgpu::ScreenDescriptor {
			size_in_pixels:   [frame.surface_size.x, frame.surface_size.y],
			pixels_per_point: match wayland_data.scale_factor {
				// TODO: Is this correct?
				Some(scale_factor) => scale_factor as f32,
				None => 1.0,
			},
		};
		let buffers = self.renderer.update_buffers(
			&wgpu_renderer.device,
			&wgpu_renderer.queue,
			&mut frame.encoder,
			&paint_jobs,
			&screen_descriptor,
		);
		let _: wgpu::SubmissionIndex = wgpu_renderer.queue.submit(buffers);

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

		full_output.platform_output
	}
}
