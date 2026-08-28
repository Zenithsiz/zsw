//! Renderer

use {
	crate::{
		menu::Menu,
		panel::{PanelState, Panels, PanelsRenderer},
		profile::ProfileName,
		shared::{Shared, SharedWindow},
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
	winit::{event::WindowEvent, window::WindowId},
	zsw_egui::Egui,
	zsw_util::AppError,
	zsw_wgpu::{FrameRender, WgpuRenderer},
};

/// Renderer
pub struct Renderer {
	shared:            Arc<Shared>,
	renderer_event_rx: mpsc::Receiver<Event>,
	menu:              Menu,

	panels: Panels,

	windows: HashMap<WindowId, WindowRenderer>,
}

impl Renderer {
	pub fn new(shared: Arc<Shared>, renderer_event_rx: mpsc::Receiver<Event>, menu: Menu) -> Result<Self, AppError> {
		let mut panels = Panels::new();
		if let Some(default_profile_name) = &shared.config.default.profile {
			let default_profile_name = default_profile_name.parse::<ProfileName>().into_ok();
			let default_profile = shared
				.profiles
				.get(&default_profile_name)
				.with_context(|| format!("Unknown profile {:?}", shared.config.default.profile))?;
			panels
				.set_profile(default_profile_name, default_profile, &shared.playlists)
				.context("Unable to set profile")?;
		}

		Ok(Self {
			shared,
			renderer_event_rx,
			menu,
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
					window_renderer.render(&self.shared, &mut self.panels, &mut self.menu)?
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
					WindowEvent::Resized(size) => {
						window_renderer
							.wgpu_renderer
							.resize(&self.shared.wgpu, size)
							.context("Unable to resize wgpu")?;
						window_renderer
							.panels_renderer
							.resize(&window_renderer.wgpu_renderer, &self.shared.wgpu, size)
					},
					WindowEvent::Moved(pos) => {
						window_renderer.shared_window.monitor_geometry.pos = euclid::point2(pos.x, pos.y);
					},
					_ => (),
				}
			},

			Event::WindowAdd { app_window } => {
				let window_renderer =
					WindowRenderer::new(&self.shared, app_window).context("Unable to create window")?;
				let window_id = window_renderer.shared_window.window.id();
				if self.windows.insert(window_id, window_renderer).is_some() {
					tracing::warn!(?window_id, "Window was re-created without being destroyed first");
				}
			},
		}


		Ok(())
	}
}

#[derive(Debug)]
struct WindowRenderer {
	shared_window:   SharedWindow,
	wgpu_renderer:   WgpuRenderer,
	panels_renderer: PanelsRenderer,
	egui:            Egui,

	next_frame:     Instant,
	frame_duration: Duration,
}

impl WindowRenderer {
	fn new(shared: &Shared, app_window: AppWindow) -> Result<Self, AppError> {
		let window = Arc::new(app_window.window);
		let wgpu_renderer =
			WgpuRenderer::new(window.share(), &shared.wgpu).context("Unable to create wgpu renderer")?;

		let msaa_samples = 4;
		let panels_renderer = PanelsRenderer::new(&wgpu_renderer, &shared.wgpu, msaa_samples)
			.context("Unable to create panels renderer")?;
		let egui = Egui::new(&shared.wgpu, &wgpu_renderer, window.share());

		let shared_window = SharedWindow {
			window,
			monitor_name: app_window.monitor_name,
			monitor_geometry: app_window.monitor_geometry,
			monitor_refresh_rate_mhz: app_window.monitor_refresh_rate_mhz,
		};

		let frame_duration = Duration::from_secs_f64(1000.0) / shared_window.monitor_refresh_rate_mhz;
		tracing::info!(
			"Window {:?} refresh rate: {:.2} Hz",
			shared_window.monitor_name,
			f64::from(shared_window.monitor_refresh_rate_mhz) / 1000.0,
		);
		tracing::info!(
			"Window {:?} frame duration: {frame_duration:.2?}",
			shared_window.monitor_name
		);

		Ok(Self {
			shared_window,
			wgpu_renderer,
			panels_renderer,
			egui,
			next_frame: Instant::now(),
			frame_duration,
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
	pub fn render(&mut self, shared: &Shared, panels: &mut Panels, menu: &mut Menu) -> Result<(), AppError> {
		let mut frame = self
			.wgpu_renderer
			.start_render(&shared.wgpu)
			.context("Unable to start frame")?;

		self.panels_renderer
			.render(
				shared,
				&self.shared_window,
				&mut frame,
				&self.wgpu_renderer,
				panels,
				&shared.panels_renderer_shared,
			)
			.context("Unable to render panels")?;

		self.render_egui(shared, panels, menu, &mut frame);

		self.wgpu_renderer
			.finish_render(&shared.wgpu, frame)
			.context("Unable to finish frame")?;

		Ok(())
	}

	/// Renders egui
	fn render_egui(&mut self, shared: &Shared, panels: &mut Panels, menu: &mut Menu, frame: &mut FrameRender) {
		self.egui
			.render(frame, &self.shared_window.window, &shared.wgpu, |ctx| {
				// Draw the menu
				menu.draw(
					ctx,
					&shared.wgpu,
					&shared.playlists,
					&shared.profiles,
					panels,
					&shared.event_loop_proxy,
					self.shared_window.monitor_geometry,
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
						!panel.geometries.iter().any(|geometry| {
							geometry
								.rect
								.on_window(self.shared_window.monitor_geometry)
								.contains(pointer_pos)
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
							PanelState::Fade(state) => state.skip(&shared.wgpu),
							PanelState::Slide(_) => (),
						}
					}

					// Scroll panels
					let scroll_delta = ctx.input(|input| input.smooth_scroll_delta.y);
					if scroll_delta != 0.0 {
						#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
						match &mut panel.state {
							PanelState::None(_) => (),
							PanelState::Fade(state) => {
								// TODO: Make this "speed" configurable
								// TODO: Perform the conversion better without going through nanos
								let speed = 1.0 / 1000.0;
								let time_delta_abs = state.duration().mul_f32(scroll_delta.abs() * speed);
								let time_delta_abs =
									TimeDelta::from_std(time_delta_abs).expect("Offset didn't fit into time delta");
								let time_delta = match scroll_delta.is_sign_positive() {
									true => -time_delta_abs,
									false => time_delta_abs,
								};

								state.step(&shared.wgpu, time_delta);
							},
							PanelState::Slide(_) => (),
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
