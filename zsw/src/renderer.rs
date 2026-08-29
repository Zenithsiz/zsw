//! Renderer

use {
	crate::{
		AppEvent,
		config::Config,
		menu::Menu,
		panel::{PanelState, Panels, PanelsRenderer, PanelsRendererShared},
		playlist::Playlists,
		profile::{ProfileName, Profiles},
		window::AppWindow,
	},
	app_error::Context,
	chrono::TimeDelta,
	core::{clone::Share, time::Duration},
	euclid::default::Point2D,
	std::{
		collections::HashMap,
		sync::{Arc, mpsc},
		thread,
		time::Instant,
	},
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

	windows: HashMap<WindowId, WindowRenderer>,
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
			windows: HashMap::new(),
		})
	}

	/// Renders all windows
	pub fn render(&mut self) -> Result<(), AppError> {
		loop {
			while let Ok(event) = self.renderer_event_rx.try_recv() {
				self.handle_event(event)?;
			}

			match self
				.windows
				.values_mut()
				.min_by_key(|window_renderer| window_renderer.next_frame)
			{
				Some(window_renderer) => {
					window_renderer.sleep_until_next_frame();
					window_renderer.render(
						&self.wgpu,
						&mut self.panels,
						&self.panels_renderer_shared,
						&self.playlists,
						&self.profiles,
						&self.event_loop_proxy,
					)?;
				},
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
				let Some(window_renderer) = self.windows.get_mut(&window_id) else {
					tracing::warn!(?window_id, ?event, "Unknown window id for event");
					return Ok(());
				};
				if window_renderer.egui.handle_event(&event) {
					return Ok(());
				}

				// TODO: When resizing we receive many `Resized` events at once,
				//       and we should only resize after the last we receive to
				//       avoid lagging while dragging.
				match event {
					WindowEvent::Resized(size) => window_renderer.queued_resize = Some(size),
					WindowEvent::Moved(pos) => window_renderer.monitor_geometry.pos = euclid::point2(pos.x, pos.y),
					_ => (),
				}
			},

			Event::WindowAdd { app_window } => {
				let window_renderer = WindowRenderer::new(&self.wgpu, app_window).context("Unable to create window")?;
				let window_id = window_renderer.window.id();
				if self.windows.insert(window_id, window_renderer).is_some() {
					tracing::warn!(?window_id, "Window was re-created without being destroyed first");
				}
			},
		}


		Ok(())
	}
}

// TODO: Package some of these together
#[derive(Debug)]
struct WindowRenderer {
	window:                    Arc<Window>,
	_monitor_name:             String,
	monitor_geometry:          Rect<i32, u32>,
	_monitor_refresh_rate_mhz: u32,

	wgpu_renderer:   WgpuRenderer,
	panels_renderer: PanelsRenderer,
	egui:            Egui,
	menu:            Menu,

	next_frame:     Instant,
	frame_duration: Duration,

	queued_resize: Option<PhysicalSize<u32>>,
}

impl WindowRenderer {
	fn new(wgpu: &Wgpu, app_window: AppWindow) -> Result<Self, AppError> {
		let window = Arc::new(app_window.window);
		let wgpu_renderer = WgpuRenderer::new(window.share(), wgpu).context("Unable to create wgpu renderer")?;

		let msaa_samples = 4;
		let panels_renderer =
			PanelsRenderer::new(&wgpu_renderer, wgpu, msaa_samples).context("Unable to create panels renderer")?;
		let egui = Egui::new(wgpu, &wgpu_renderer, window.share());

		let frame_duration = Duration::from_secs_f64(1000.0) / app_window.monitor_refresh_rate_mhz;
		tracing::info!(
			"Window {:?} refresh rate: {:.2} Hz",
			app_window.monitor_name,
			f64::from(app_window.monitor_refresh_rate_mhz) / 1000.0,
		);
		tracing::info!(
			"Window {:?} frame duration: {frame_duration:.2?}",
			app_window.monitor_name
		);

		Ok(Self {
			window,
			_monitor_name: app_window.monitor_name,
			monitor_geometry: app_window.monitor_geometry,
			_monitor_refresh_rate_mhz: app_window.monitor_refresh_rate_mhz,
			wgpu_renderer,
			panels_renderer,
			egui,
			menu: Menu::new(),
			next_frame: Instant::now(),
			frame_duration,
			queued_resize: None,
		})
	}

	/// Sleeps until the next frame and prepares for it.
	fn sleep_until_next_frame(&mut self) {
		let prev_frame_end = Instant::now();
		let cur_frame_start = self.next_frame;
		self.next_frame += self.frame_duration;

		// Wait until the start of the next frame.
		// Note: We do this instead of letting wgpu sleep because all vsync
		//       modes that sleep for us do so with a 3 frame buffer, which
		//       means that the user is always 3 frames behind, and thus can
		//       notice pretty heavy lag (at 60Hz, about 50 ms).
		//       We also set the present mode to mailbox, which means that we
		//       don't get any tearing, but have minimal lag.
		thread::sleep_until(cur_frame_start);

		// If we were too late, we need to skip some frames
		if let Some(late) = prev_frame_end.checked_duration_since(cur_frame_start) &&
			late > self.frame_duration
		{
			let frames = late.div_duration_floor(self.frame_duration);
			tracing::trace!("Frame rendered late {late:.2?}, skipping {frames} frames");

			// Note: This isn't as paranoic as it seems, since if the user sets the frame duration
			//       to 1 ns by setting their frame rate to infinite, then this would fail if we're
			//       late 5 seconds (`log2(5s / 1ns) ~ 32.2`). At which point we just give up on
			//       keeping the frame timing and reset it to now instead.
			match u32::try_from(frames) {
				Ok(frames) => self.next_frame += self.frame_duration * frames,
				Err(_) => self.next_frame = Instant::now(),
			}
		}
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
			self.panels_renderer.resize(&self.wgpu_renderer, wgpu, size)
		}

		let mut frame = self.wgpu_renderer.start_render(wgpu).context("Unable to start frame")?;

		self.panels_renderer
			.render(
				wgpu,
				self.monitor_geometry,
				&mut frame,
				&self.wgpu_renderer,
				panels,
				panels_renderer_shared,
			)
			.context("Unable to render panels")?;

		self.render_egui(wgpu, panels, playlists, profiles, event_loop_proxy, &mut frame);

		self.wgpu_renderer
			.finish_render(wgpu, frame)
			.context("Unable to finish frame")?;

		Ok(())
	}

	/// Renders egui
	fn render_egui(
		&mut self,
		wgpu: &Wgpu,
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
				self.monitor_geometry,
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
						.any(|geometry| geometry.rect.on_window(self.monitor_geometry).contains(pointer_pos))
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
	WindowAdd { app_window: AppWindow },
}
