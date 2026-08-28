//! Renderer

// Imports
use {
	crate::{
		menu::Menu,
		panel::{PanelState, PanelsRenderer},
		shared::{Shared, SharedWindow},
	},
	app_error::Context,
	chrono::TimeDelta,
	core::time::Duration,
	euclid::default::Point2D,
	std::{
		sync::{Arc, mpsc},
		thread,
		time::Instant,
	},
	winit::event::WindowEvent,
	zsw_egui::Egui,
	zsw_util::AppError,
	zsw_wgpu::{FrameRender, WgpuRenderer},
};

/// Renderer
pub struct Renderer {
	shared:            Arc<Shared>,
	shared_window:     SharedWindow,
	renderer_event_rx: mpsc::Receiver<Event>,
	wgpu_renderer:     WgpuRenderer,
	panels_renderer:   PanelsRenderer,
	egui:              Egui,
	menu:              Menu,

	next_frame:     Instant,
	frame_duration: Duration,
}

impl Renderer {
	pub fn new(
		shared: Arc<Shared>,
		shared_window: SharedWindow,
		renderer_event_rx: mpsc::Receiver<Event>,
		wgpu_renderer: WgpuRenderer,
		panels_renderer: PanelsRenderer,
		egui: Egui,
		menu: Menu,
	) -> Self {
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

		Self {
			shared,
			shared_window,
			renderer_event_rx,
			wgpu_renderer,
			panels_renderer,
			egui,
			menu,
			next_frame: Instant::now(),
			frame_duration,
		}
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

	/// Renders the next frame.
	pub fn render(&mut self) -> Result<(), AppError> {
		self.sleep_until_next_frame();

		// Start rendering
		let mut frame = self
			.wgpu_renderer
			.start_render(&self.shared.wgpu)
			.context("Unable to start frame")?;

		// Render panels
		self.panels_renderer
			.render(
				&self.shared,
				&mut self.shared_window,
				&mut frame,
				&self.wgpu_renderer,
				&self.shared.panels_renderer_shared,
			)
			.context("Unable to render panels")?;

		// Render egui
		self.render_egui(&mut frame);

		// Finish the frame
		if frame.finish(&self.shared.wgpu) {
			self.wgpu_renderer
				.reconfigure(&self.shared.wgpu)
				.context("Unable to reconfigure wgpu")?;
		}

		self.handle_events()?;

		Ok(())
	}

	/// Handles all events
	fn handle_events(&mut self) -> Result<(), AppError> {
		// Handle events
		let mut resize = None;
		let mut move_pos = None;

		while let Ok(event) = self.renderer_event_rx.try_recv() {
			tracing::trace!("Received renderer event: {event:?}");
			match event {
				Event::WindowEvent { event } => {
					if self.egui.handle_event(&event) {
						continue;
					}

					match event {
						WindowEvent::Resized(size) => resize = Some(size),
						WindowEvent::Moved(pos) => move_pos = Some(pos),
						_ => (),
					}
				},
			}
		}

		// Note: When resizing we might receive multiple resize events
		//       per frame, so we only use the latest one from the events,
		//       since resizing twice only has the affect of the last resize.
		if let Some(size) = resize {
			self.wgpu_renderer
				.resize(&self.shared.wgpu, size)
				.context("Unable to resize wgpu")?;
			self.panels_renderer
				.resize(&self.wgpu_renderer, &self.shared.wgpu, size)
		}
		if let Some(pos) = move_pos {
			self.shared_window.monitor_geometry.pos = euclid::point2(pos.x, pos.y);
		}

		Ok(())
	}

	/// Renders egui
	fn render_egui(&mut self, frame: &mut FrameRender) {
		self.egui
			.render(frame, &self.shared_window.window, &self.shared.wgpu, |ctx| {
				// Draw the menu
				self.menu.draw(
					ctx,
					&self.shared.wgpu,
					&self.shared.playlists,
					&self.shared.profiles,
					&mut self.shared_window.panels,
					&self.shared.event_loop_proxy,
					self.shared_window.monitor_geometry,
				);


				// Then go through all panels checking for interactions with their geometries
				// TODO: Should this be done here and not somewhere else?
				let Some(pointer_pos) = ctx.input(|input| input.pointer.latest_pos()) else {
					return;
				};
				let pointer_pos = Point2D::new(pointer_pos.x as i32, pointer_pos.y as i32);
				for panel in self.shared_window.panels.get_all() {
					// If we're over an egui area, or none of the geometries are underneath the cursor, skip the panel
					if ctx.is_pointer_over_egui() ||
						!panel.geometries.iter().any(|geometry| {
							geometry
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
							PanelState::Fade(state) => state.skip(&self.shared.wgpu),
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

								state.step(&self.shared.wgpu, time_delta);
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
	WindowEvent { event: WindowEvent },
}
