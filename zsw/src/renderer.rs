//! Renderer

use {
	crate::{
		AppEvent,
		config::Config,
		menu::Menu,
		panel::{PanelState, Panels, PanelsRenderer, PanelsRendererShared},
		playlist::Playlists,
		profile::{ProfileName, Profiles},
	},
	app_error::Context,
	chrono::TimeDelta,
	core::clone::Share,
	euclid::default::Point2D,
	std::sync::{Arc, mpsc},
	winit::{
		dpi::PhysicalSize,
		event::WindowEvent,
		event_loop::EventLoopProxy,
		window::{Window, WindowId},
	},
	zsw_egui::Egui,
	zsw_util::{AppError, Rect},
	zsw_wgpu::{FrameRender, Wgpu, WgpuRenderer},
};

/// Renderer
// TODO: Package some of these together
pub struct Renderer {
	event_loop_proxy: EventLoopProxy<AppEvent>,

	wgpu:                   Wgpu,
	panels_renderer_shared: PanelsRendererShared,

	playlists: Playlists,
	profiles:  Profiles,

	renderer_event_rx: mpsc::Receiver<Event>,

	panels: Panels,

	window_renderer: Option<WindowRenderer>,
}

impl Renderer {
	pub fn new(
		config: &Config,
		event_loop_proxy: EventLoopProxy<AppEvent>,
		wgpu: Wgpu,
		panels_renderer_shared: PanelsRendererShared,
		playlists: Playlists,
		profiles: Profiles,
		renderer_event_rx: mpsc::Receiver<Event>,
	) -> Result<Self, AppError> {
		let mut panels = Panels::new();
		if let Some(default_profile_name) = &config.default.profile {
			let default_profile_name = default_profile_name.parse::<ProfileName>().into_ok();
			let default_profile = profiles
				.get(&default_profile_name)
				.with_context(|| format!("Unknown profile {:?}", config.default.profile))?;
			panels
				.set_profile(default_profile_name, default_profile, &playlists)
				.context("Unable to set profile")?;
		}

		Ok(Self {
			event_loop_proxy,
			wgpu,
			panels_renderer_shared,
			playlists,
			profiles,
			renderer_event_rx,
			panels,
			window_renderer: None,
		})
	}

	/// Renders all windows
	pub fn render(&mut self) -> Result<(), AppError> {
		loop {
			while let Ok(event) = self.renderer_event_rx.try_recv() {
				self.handle_event(event)?;
			}

			match &mut self.window_renderer {
				Some(window_renderer) => window_renderer.render(
					&self.wgpu,
					&mut self.panels,
					&self.panels_renderer_shared,
					&self.playlists,
					&self.profiles,
					&self.event_loop_proxy,
				)?,
				None => {
					let Ok(event) = self.renderer_event_rx.recv() else {
						break;
					};
					self.handle_event(event)?;
				},
			}
		}

		Ok(())
	}

	/// Handles an event
	fn handle_event(&mut self, event: Event) -> Result<(), AppError> {
		tracing::trace!("Received renderer event: {event:?}");
		match event {
			Event::WindowEvent { window_id, event } => {
				let Some(window_renderer) = &mut self.window_renderer else {
					tracing::warn!(?window_id, ?event, "Received a window event with no active window");
					return Ok(());
				};
				if window_renderer.egui.handle_event(&event) {
					return Ok(());
				}

				#[expect(clippy::single_match, reason = "We'll add more in the future")]
				match event {
					WindowEvent::Resized(size) => window_renderer.queued_resize = Some(size),
					_ => (),
				}
			},

			Event::WindowAdd { window } => {
				let window_renderer = WindowRenderer::new(&self.wgpu, window).context("Unable to create window")?;
				self.window_renderer = Some(window_renderer);
			},
		}


		Ok(())
	}
}

// TODO: Package some of these together
#[derive(Debug)]
struct WindowRenderer {
	window:      Arc<Window>,
	window_size: PhysicalSize<u32>,

	wgpu_renderer:   WgpuRenderer,
	panels_renderer: PanelsRenderer,
	egui:            Egui,
	menu:            Menu,

	queued_resize: Option<PhysicalSize<u32>>,
}

impl WindowRenderer {
	fn new(wgpu: &Wgpu, window: Window) -> Result<Self, AppError> {
		let window = Arc::new(window);
		let wgpu_renderer = WgpuRenderer::new(window.share(), wgpu).context("Unable to create wgpu renderer")?;

		let msaa_samples = 4;
		let panels_renderer =
			PanelsRenderer::new(&wgpu_renderer, wgpu, msaa_samples).context("Unable to create panels renderer")?;
		let egui = Egui::new(wgpu, &wgpu_renderer, window.share());

		Ok(Self {
			window,
			// Note: We typically always get a resize event before the first
			//       frame, so this size isn't ever visible.
			window_size: PhysicalSize::new(0, 0),
			wgpu_renderer,
			panels_renderer,
			egui,
			menu: Menu::new(),
			queued_resize: None,
		})
	}

	/// Renders the current frame.
	///
	/// Does not check whether it is time for it or not, you must
	/// instead call [`Self::sleep_until_next_frame`] and/or check
	/// [`Self::next_frame`].
	pub fn render(
		&mut self,
		wgpu: &Wgpu,
		panels: &mut Panels,
		panels_renderer_shared: &PanelsRendererShared,
		playlists: &Playlists,
		profiles: &Profiles,
		event_loop_proxy: &EventLoopProxy<AppEvent>,
	) -> Result<(), AppError> {
		// If we need to resize, do it now
		if let Some(size) = self.queued_resize.take() {
			self.wgpu_renderer.resize(wgpu, size).context("Unable to resize wgpu")?;
			self.panels_renderer.resize(&self.wgpu_renderer, wgpu, size);
			self.window_size = size;
		}

		let window_geometry = Rect {
			pos:  euclid::point2(0, 0),
			size: euclid::vec2(self.window_size.width, self.window_size.height),
		};

		let mut frame = self.wgpu_renderer.start_render(wgpu).context("Unable to start frame")?;

		self.panels_renderer
			.render(
				wgpu,
				window_geometry,
				&mut frame,
				&self.wgpu_renderer,
				panels,
				panels_renderer_shared,
			)
			.context("Unable to render panels")?;

		self.render_egui(
			wgpu,
			window_geometry,
			panels,
			playlists,
			profiles,
			event_loop_proxy,
			&mut frame,
		);

		self.wgpu_renderer
			.finish_render(wgpu, frame)
			.context("Unable to finish frame")?;

		Ok(())
	}

	/// Renders egui
	fn render_egui(
		&mut self,
		wgpu: &Wgpu,
		window_geometry: Rect<i32, u32>,
		panels: &mut Panels,
		playlists: &Playlists,
		profiles: &Profiles,
		event_loop_proxy: &EventLoopProxy<AppEvent>,
		frame: &mut FrameRender,
	) {
		self.egui.render(frame, &self.window, wgpu, |ctx| {
			// Draw the menu
			self.menu.draw(
				ctx,
				wgpu,
				playlists,
				profiles,
				panels,
				event_loop_proxy,
				window_geometry,
			);

			// Then go through all panels checking for interactions with their geometries
			// TODO: Should this be done here and not somewhere else?
			let Some(pointer_pos) = ctx.input(|input| input.pointer.latest_pos()) else {
				return;
			};
			let pointer_pos = Point2D::new(pointer_pos.x as i32, pointer_pos.y as i32);
			for panel in panels.get_all() {
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
						PanelState::Fade(state) => state.skip(wgpu),
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
						PanelState::Fade(state) => state.step(wgpu, time_delta),
						PanelState::Slide(state) => state.step(wgpu, time_delta),
					}
				}
			}
		})
	}
}

/// Renderer event
#[derive(Debug)]
pub enum Event {
	/// Window event
	WindowEvent {
		window_id: WindowId,
		event:     WindowEvent,
	},

	/// Add new window
	WindowAdd { window: Window },
}
