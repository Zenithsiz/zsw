//! Renderer

use {
	crate::{
		Zsw,
		menu::Menu,
		panel::{self, Panel, Panels},
		playlist::Playlists,
		profile::Profiles,
	},
	app_error::Context,
	chrono::TimeDelta,
	core::time::Duration,
	euclid::default::{Point2D, Vector2D},
	std::sync::Arc,
	zsw_egui::Egui,
	zsw_util::{AppError, Rect},
	zsw_wayland::WaylandData,
	zsw_wgpu::{FrameRender, RenderedFrame, Wgpu, WgpuRenderer},
};

#[derive(Debug)]
pub struct SurfaceRenderer {
	surface_geometry: Rect<i32, u32>,

	wgpu_renderer:   WgpuRenderer,
	panels_renderer: panel::Renderer,
	egui:            Egui,
	menu:            Menu,

	queued_resize: Option<Vector2D<u32>>,
}

impl SurfaceRenderer {
	/// Creates the surface renderer
	pub fn new(
		wgpu: &Wgpu,
		target: zsw_wgpu::SurfaceTarget,
		surface_geometry: Rect<i32, u32>,
	) -> Result<Self, AppError> {
		let wgpu_renderer =
			WgpuRenderer::new(wgpu, target, surface_geometry.size).context("Unable to create wgpu renderer")?;

		let msaa_samples = 4;
		let panels_renderer =
			panel::Renderer::new(wgpu, &wgpu_renderer, msaa_samples).context("Unable to create panels renderer")?;
		let egui = Egui::new(wgpu, &wgpu_renderer);

		Ok(Self {
			surface_geometry,
			wgpu_renderer,
			panels_renderer,
			egui,
			menu: Menu::new(),
			queued_resize: None,
		})
	}

	/// Queues a resize to this renderer
	///
	/// This will stay queued for the next render
	pub fn queue_resize(&mut self, size: Vector2D<u32>) {
		self.queued_resize = Some(size);
	}

	/// Starts a frame
	///
	/// Performs any queued resize
	pub fn start_frame(&mut self, wgpu: &Wgpu) -> Result<FrameRender, AppError> {
		// If we need to resize, do it now before starting the new frame
		if let Some(size) = self.queued_resize.take() {
			self.wgpu_renderer.resize(wgpu, size).context("Unable to resize wgpu")?;
			self.panels_renderer.resize(wgpu, &self.wgpu_renderer, size);
			self.surface_geometry.size = size;
		}

		self.wgpu_renderer.start_frame(wgpu).context("Unable to start frame")
	}

	/// Renders the a frame.
	///
	/// You can get the current frame from [`start_frame`](Self::start_frame).
	#[expect(clippy::too_many_arguments, reason = "TODO: Package some together")]
	pub fn render(
		&mut self,
		wgpu: &Arc<Wgpu>,
		wayland_data: &mut WaylandData<Zsw>,
		playlists: &Playlists,
		profiles: &Profiles,
		panels: &mut Panels,
		egui_input: egui::RawInput,
		frame: &mut FrameRender,
		delta: Duration,
	) -> Result<egui::PlatformOutput, AppError> {
		self.panels_renderer
			.render(wgpu, &self.wgpu_renderer, self.surface_geometry, frame, panels, delta)
			.context("Unable to render panels")?;

		let egui_output = self.render_egui(
			wgpu,
			wayland_data,
			self.surface_geometry,
			playlists,
			profiles,
			panels,
			egui_input,
			frame,
		);

		Ok(egui_output)
	}

	/// Ends a frame
	pub fn submit_frame(&mut self, wgpu: &Wgpu, frame: FrameRender) -> Result<RenderedFrame, AppError> {
		self.wgpu_renderer
			.submit_frame(wgpu, frame)
			.context("Unable to finish frame")
	}

	/// Presents a frame
	pub fn present_frame(&mut self, wgpu: &Wgpu, frame: RenderedFrame) -> Result<(), AppError> {
		self.wgpu_renderer
			.present_frame(wgpu, frame)
			.context("Unable to finish frame")?;

		Ok(())
	}

	/// Renders egui
	#[expect(clippy::too_many_arguments, reason = "TODO: Package some together")]
	fn render_egui(
		&mut self,
		wgpu: &Arc<Wgpu>,
		wayland_data: &mut WaylandData<Zsw>,
		surface_geometry: Rect<i32, u32>,
		playlists: &Playlists,
		profiles: &Profiles,
		panels: &mut Panels,
		egui_input: egui::RawInput,
		frame: &mut FrameRender,
	) -> egui::PlatformOutput {
		let output = self.egui.paint(egui_input, |ctx| {
			// Draw the menu
			self.menu
				.draw(ctx, wayland_data, wgpu, playlists, profiles, panels, surface_geometry);

			// Then go through all panels checking for interactions with their geometries
			// TODO: Should this be done here and not somewhere else?
			let Some(pointer_pos) = ctx.input(|input| input.pointer.latest_pos()) else {
				return;
			};
			let pointer_pos = Point2D::new(pointer_pos.x as i32, pointer_pos.y as i32);
			for panel in panels.get_all() {
				// If we're over an egui area, or none of the geometries are underneath the cursor, skip the panel
				let pointer_pos_on_surface = pointer_pos + surface_geometry.pos.to_vector();
				if ctx.is_pointer_over_egui() || !panel.any_contain(pointer_pos_on_surface) {
					continue;
				}

				// Pause any double-clicked panels
				if ctx.input(|input| input.pointer.button_double_clicked(egui::PointerButton::Primary)) {
					#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
					match panel {
						Panel::None(_) => (),
						Panel::Fade(shader) => shader.toggle_paused(),
						Panel::Slide(_) => (),
					}
				}

				// Skip any ctrl-clicked/middle clicked panels
				if ctx.input(|input| {
					(input.pointer.button_clicked(egui::PointerButton::Primary) && input.modifiers.ctrl) ||
						input.pointer.button_clicked(egui::PointerButton::Middle)
				}) {
					#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
					match panel {
						Panel::None(_) => (),
						Panel::Fade(shader) => shader.skip(wgpu),
						Panel::Slide(_) => (),
					}
				}

				// Scroll panels
				let scroll_delta = ctx.input(|input| input.smooth_scroll_delta.y);
				if scroll_delta != 0.0 {
					let time_delta = match panel {
						Panel::None(_) => TimeDelta::zero(),
						Panel::Fade(shader) => {
							// TODO: Make this "speed" configurable
							// TODO: Perform the conversion better without going through nanos
							let speed = 1.0 / 1000.0;
							let time_delta_abs = shader.duration().mul_f32(scroll_delta.abs() * speed);
							let time_delta_abs =
								TimeDelta::from_std(time_delta_abs).expect("Offset didn't fit into time delta");
							match scroll_delta.is_sign_positive() {
								true => -time_delta_abs,
								false => time_delta_abs,
							}
						},
						Panel::Slide(shader) => {
							// TODO: Make this "speed" configurable
							// TODO: Perform the conversion better without going through nanos
							let speed = 1.0 / 1000.0;
							let time_delta_abs = shader.duration().mul_f32(scroll_delta.abs() * speed);
							let time_delta_abs =
								TimeDelta::from_std(time_delta_abs).expect("Offset didn't fit into time delta");
							match scroll_delta.is_sign_positive() {
								true => -time_delta_abs,
								false => time_delta_abs,
							}
						},
					};

					match panel {
						Panel::None(_) => (),
						Panel::Fade(shader) => shader.step(wgpu, time_delta),
						Panel::Slide(shader) => shader.step(wgpu, time_delta),
					}
				}
			}
		});

		self.egui.render(frame, wayland_data, wgpu, output)
	}
}
