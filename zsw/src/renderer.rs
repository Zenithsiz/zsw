//! Renderer

use {
	crate::{
		Zsw,
		menu::Menu,
		panel::{self, PanelState, Panels, PanelsRenderer},
		playlist::Playlists,
		profile::{ProfileName, Profiles},
	},
	app_error::Context,
	chrono::TimeDelta,
	euclid::default::{Point2D, Vector2D},
	zsw_egui::Egui,
	zsw_util::{AppError, Rect},
	zsw_wayland::WaylandData,
	zsw_wgpu::{FrameRender, RenderedFrame, WgpuRenderer},
};

#[derive(Debug)]
pub struct SurfaceRenderer {
	surface_geometry: Rect<i32, u32>,

	wgpu_renderer:   WgpuRenderer,
	panels:          Panels,
	panels_renderer: PanelsRenderer,
	egui:            Egui,
	menu:            Menu,

	queued_resize: Option<Vector2D<u32>>,
}

impl SurfaceRenderer {
	/// Creates the surface renderer
	pub async fn new(
		target: zsw_wgpu::SurfaceTarget,
		surface_geometry: Rect<i32, u32>,
		profiles: &Profiles,
		profile_name: &ProfileName,
		playlists: &Playlists,
	) -> Result<Self, AppError> {
		let wgpu_renderer = WgpuRenderer::new(target, surface_geometry.size)
			.await
			.context("Unable to create wgpu renderer")?;

		let msaa_samples = 4;
		let panels_renderer =
			PanelsRenderer::new(&wgpu_renderer, msaa_samples).context("Unable to create panels renderer")?;
		let egui = Egui::new(&wgpu_renderer);

		let mut panels = Panels::new();
		let profile = profiles
			.get(profile_name)
			.with_context(|| format!("Unknown profile {profile_name:?}"))?;
		panels
			.set_profile(profile_name.clone(), profile, playlists)
			.context("Unable to set profile")?;

		Ok(Self {
			surface_geometry,
			wgpu_renderer,
			panels,
			panels_renderer,
			egui,
			menu: Menu::new(),
			queued_resize: None,
		})
	}

	/// Returns the wgpu renderer for this surface
	pub fn wgpu_renderer(&self) -> &WgpuRenderer {
		&self.wgpu_renderer
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
	pub fn start_frame(&mut self) -> Result<FrameRender, AppError> {
		// If we need to resize, do it now before starting the new frame
		if let Some(size) = self.queued_resize.take() {
			self.wgpu_renderer.resize(size).context("Unable to resize wgpu")?;
			self.panels_renderer.resize(&self.wgpu_renderer, size);
			self.surface_geometry.size = size;
		}

		self.wgpu_renderer.start_frame().context("Unable to start frame")
	}

	/// Renders the a frame.
	///
	/// You can get the current frame from [`start_frame`](Self::start_frame).
	pub fn render(
		&mut self,
		wayland_data: &mut WaylandData<Zsw>,
		playlists: &Playlists,
		profiles: &Profiles,
		egui_input: egui::RawInput,
		frame: &mut FrameRender,
	) -> Result<egui::PlatformOutput, AppError> {
		self.panels_renderer
			.render(&self.wgpu_renderer, self.surface_geometry, frame, &mut self.panels)
			.context("Unable to render panels")?;

		let egui_output = self.render_egui(
			wayland_data,
			self.surface_geometry,
			playlists,
			profiles,
			egui_input,
			frame,
		);

		Ok(egui_output)
	}

	/// Ends a frame
	pub fn submit_frame(&mut self, frame: FrameRender) -> Result<RenderedFrame, AppError> {
		self.wgpu_renderer.submit_frame(frame).context("Unable to finish frame")
	}

	/// Presents a frame
	pub fn present_frame(&mut self, frame: RenderedFrame) -> Result<(), AppError> {
		self.wgpu_renderer
			.present_frame(frame)
			.context("Unable to finish frame")?;

		Ok(())
	}

	/// Renders egui
	fn render_egui(
		&mut self,
		wayland_data: &mut WaylandData<Zsw>,
		surface_geometry: Rect<i32, u32>,
		playlists: &Playlists,
		profiles: &Profiles,
		egui_input: egui::RawInput,
		frame: &mut FrameRender,
	) -> egui::PlatformOutput {
		let output = self.egui.paint(egui_input, |ctx| {
			// Draw the menu
			self.menu.draw(
				ctx,
				wayland_data,
				&self.wgpu_renderer,
				playlists,
				profiles,
				&mut self.panels,
				surface_geometry,
			);

			// Then go through all panels checking for interactions with their geometries
			// TODO: Should this be done here and not somewhere else?
			let Some(pointer_pos) = ctx.input(|input| input.pointer.latest_pos()) else {
				return;
			};
			let pointer_pos = Point2D::new(pointer_pos.x as i32, pointer_pos.y as i32);
			for panel in self.panels.get_all() {
				// If we're over an egui area, or none of the geometries are underneath the cursor, skip the panel
				if ctx.is_pointer_over_egui() ||
					!panel.geometries.iter().any(|geometry| {
						panel::geometry::relative_to(geometry.rect, surface_geometry).contains(pointer_pos)
					}) {
					continue;
				}

				// Pause any double-clicked panels
				if ctx.input(|input| input.pointer.button_double_clicked(egui::PointerButton::Primary)) {
					#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
					match &mut panel.state {
						PanelState::None(_) => (),
						PanelState::Fade(state) => state.toggle_paused(),
						PanelState::Slide(_) => (),
					}
				}

				// Skip any ctrl-clicked/middle clicked panels
				if ctx.input(|input| {
					(input.pointer.button_clicked(egui::PointerButton::Primary) && input.modifiers.ctrl) ||
						input.pointer.button_clicked(egui::PointerButton::Middle)
				}) {
					#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
					match &mut panel.state {
						PanelState::None(_) => (),
						PanelState::Fade(state) => state.skip(&self.wgpu_renderer),
						PanelState::Slide(_) => (),
					}
				}

				// Scroll panels
				let scroll_delta = ctx.input(|input| input.smooth_scroll_delta.y);
				if scroll_delta != 0.0 {
					let time_delta = match &panel.state {
						PanelState::None(_) => TimeDelta::zero(),
						PanelState::Fade(state) => {
							// TODO: Make this "speed" configurable
							// TODO: Perform the conversion better without going through nanos
							let speed = 1.0 / 1000.0;
							let time_delta_abs = state.duration().mul_f32(scroll_delta.abs() * speed);
							let time_delta_abs =
								TimeDelta::from_std(time_delta_abs).expect("Offset didn't fit into time delta");
							match scroll_delta.is_sign_positive() {
								true => -time_delta_abs,
								false => time_delta_abs,
							}
						},
						PanelState::Slide(state) => {
							// TODO: Make this "speed" configurable
							// TODO: Perform the conversion better without going through nanos
							let speed = 1.0 / 1000.0;
							let time_delta_abs = state.duration().mul_f32(scroll_delta.abs() * speed);
							let time_delta_abs =
								TimeDelta::from_std(time_delta_abs).expect("Offset didn't fit into time delta");
							match scroll_delta.is_sign_positive() {
								true => -time_delta_abs,
								false => time_delta_abs,
							}
						},
					};

					match &mut panel.state {
						PanelState::None(_) => (),
						PanelState::Fade(state) => state.step(&self.wgpu_renderer, time_delta),
						PanelState::Slide(state) => state.step(&self.wgpu_renderer, time_delta),
					}
				}
			}
		});

		self.egui.render(frame, wayland_data, &self.wgpu_renderer, output)
	}
}
