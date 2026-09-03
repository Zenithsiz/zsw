//! Renderer

use {
	crate::{
		AppEvent,
		menu::Menu,
		panel::{PanelState, Panels, PanelsRenderer},
		playlist::Playlists,
		profile::{ProfileName, Profiles},
	},
	app_error::Context,
	chrono::TimeDelta,
	core::clone::Share,
	euclid::default::{Point2D, Vector2D},
	std::sync::Arc,
	winit::{
		event::WindowEvent,
		event_loop::{EventLoopProxy, OwnedDisplayHandle},
		window::Window,
	},
	zsw_egui::Egui,
	zsw_util::{AppError, Rect},
	zsw_wgpu::{FrameRender, WgpuRenderer},
};

#[derive(Debug)]
pub struct WindowRenderer {
	window:      Arc<Window>,
	window_size: Vector2D<u32>,

	wgpu_renderer:   WgpuRenderer,
	panels:          Panels,
	panels_renderer: PanelsRenderer,
	egui:            Egui,
	menu:            Menu,

	queued_resize: Option<Vector2D<u32>>,
}

impl WindowRenderer {
	pub async fn new(
		display: OwnedDisplayHandle,
		window: Window,
		profiles: &Profiles,
		profile_name: &ProfileName,
		playlists: &Playlists,
	) -> Result<Self, AppError> {
		let window = Arc::new(window);
		let wgpu_renderer = WgpuRenderer::new(display, &window)
			.await
			.context("Unable to create wgpu renderer")?;

		let msaa_samples = 4;
		let panels_renderer =
			PanelsRenderer::new(&wgpu_renderer, msaa_samples).context("Unable to create panels renderer")?;
		let egui = Egui::new(&wgpu_renderer, window.share());

		let mut panels = Panels::new();
		let profile = profiles
			.get(profile_name)
			.with_context(|| format!("Unknown profile {profile_name:?}"))?;
		panels
			.set_profile(&wgpu_renderer, profile_name.clone(), profile, playlists)
			.context("Unable to set profile")?;

		Ok(Self {
			window,
			// Note: We typically always get a resize event before the first
			//       frame, so this size isn't ever visible.
			window_size: Vector2D::new(0, 0),
			wgpu_renderer,
			panels,
			panels_renderer,
			egui,
			menu: Menu::new(),
			queued_resize: None,
		})
	}

	/// Returns the window this renderer is using
	pub fn window(&self) -> &Window {
		&self.window
	}

	/// Forwards a window event to egui.
	///
	/// Returns if egui wants exclusive use of that event
	pub fn forward_egui_window_event(&mut self, event: &WindowEvent) -> bool {
		self.egui.handle_event(event)
	}

	/// Queues a resize for the next render
	pub fn queue_resize(&mut self, size: Vector2D<u32>) {
		self.queued_resize = Some(size);
	}

	/// Waits until the next frame.
	///
	/// Performs any queued resize
	pub fn wait_frame(&mut self) -> Result<FrameRender, AppError> {
		// If we need to resize, do it now before starting the new frame
		if let Some(size) = self.queued_resize.take() {
			self.wgpu_renderer.resize(size).context("Unable to resize wgpu")?;
			self.panels_renderer.resize(&self.wgpu_renderer, size);
			self.window_size = size;
		}

		self.wgpu_renderer.start_render().context("Unable to start frame")
	}

	/// Renders the a frame.
	///
	/// You can get the current frame from [`wait_frame`](Self::wait_frame).
	pub fn render(
		&mut self,
		playlists: &Playlists,
		profiles: &Profiles,
		event_loop_proxy: &EventLoopProxy<AppEvent>,
		mut frame: FrameRender,
	) -> Result<(), AppError> {
		let window_geometry = Rect {
			pos:  euclid::point2(0, 0),
			size: self.window_size,
		};

		self.panels_renderer
			.render(&self.wgpu_renderer, window_geometry, &mut frame, &mut self.panels)
			.context("Unable to render panels")?;

		self.render_egui(window_geometry, playlists, profiles, event_loop_proxy, &mut frame);

		self.wgpu_renderer
			.finish_render(frame)
			.context("Unable to finish frame")?;

		Ok(())
	}

	/// Renders egui
	fn render_egui(
		&mut self,
		window_geometry: Rect<i32, u32>,
		playlists: &Playlists,
		profiles: &Profiles,
		event_loop_proxy: &EventLoopProxy<AppEvent>,
		frame: &mut FrameRender,
	) {
		self.egui.render(frame, &self.window, &self.wgpu_renderer, |ctx| {
			// Draw the menu
			self.menu.draw(
				ctx,
				&self.wgpu_renderer,
				playlists,
				profiles,
				&mut self.panels,
				event_loop_proxy,
				window_geometry,
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
					!panel
						.geometries
						.iter()
						.any(|geometry| geometry.rect.on_window(window_geometry).contains(pointer_pos))
				{
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
		})
	}
}
