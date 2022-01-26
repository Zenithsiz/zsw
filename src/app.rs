//! App
//!
//! See the [`App`] type for more details

// Imports
use crate::{paths, util, Args, Egui, ImageLoader, Panel, PanelState, PanelsProfile, PanelsRenderer, Rect, Wgpu};
use anyhow::Context;
use cgmath::{Point2, Vector2};
use crossbeam::atomic::AtomicCell;
use egui::Widget;
use parking_lot::Mutex;
use std::{mem, num::NonZeroUsize, thread, time::Duration};
use winit::{
	dpi::{PhysicalPosition, PhysicalSize},
	event::{Event, WindowEvent},
	event_loop::{ControlFlow as EventLoopControlFlow, EventLoop},
	platform::{
		run_return::EventLoopExtRunReturn,
		unix::{WindowBuilderExtUnix, WindowExtUnix, XWindowType},
	},
	window::{Window, WindowBuilder},
};
use x11::xlib;

/// Inner state
// Note: This is required because the event loop can't be
//       shared in between threads, but everything else we need
//       to share.
#[derive(Debug)]
struct Inner {
	/// Window
	///
	/// [`Wgpu`] required a static reference to it, which is
	/// why we leak it.
	window: &'static Window,

	/// Wgpu
	wgpu: Wgpu,

	/// Path distributer
	paths_distributer: paths::Distributer,

	/// Image loader
	image_loader: ImageLoader,

	/// Panels
	panels: Mutex<Vec<Panel>>,

	/// Panels renderer
	panels_renderer: PanelsRenderer,

	/// Egui
	egui: Egui,

	/// Queued settings window open click
	queued_settings_window_open_click: AtomicCell<Option<PhysicalPosition<f64>>>,

	/// If the settings window is currently open
	settings_window_open: Mutex<bool>,

	/// New panel parameters
	new_panel_parameters: Mutex<(Rect<u32>, f32, f32)>,

	/// Profiles
	_profiles: Vec<PanelsProfile>,
}

/// Application state
///
/// Stores all of the application state
#[derive(Debug)]
pub struct App {
	/// Event loop
	event_loop: EventLoop<!>,

	/// Inner
	inner: Inner,
}

impl App {
	/// Creates a new app
	#[allow(clippy::future_not_send)] // Unfortunately we can't do much about it, we must build the window in the main thread
	pub async fn new(args: Args) -> Result<Self, anyhow::Error> {
		// Build the window
		let (event_loop, window) = self::create_window(&args)?;

		// Create the wgpu interface
		let wgpu = Wgpu::new(window).await.context("Unable to create renderer")?;

		// Create the paths channel
		let (paths_distributer, paths_rx) = paths::new(args.images_dir);

		// Create the image loader
		let image_loader = ImageLoader::new(paths_rx).context("Unable to create image loader")?;

		// Create all panels
		let panels = args
			.panel_geometries
			.iter()
			.map(|&geometry| Panel::new(geometry, PanelState::Empty, args.image_duration, args.fade_point))
			.collect::<Vec<_>>();
		let panels = Mutex::new(panels);

		// Create the panels renderer
		let panels_renderer = PanelsRenderer::new(wgpu.device(), wgpu.surface_texture_format())
			.await
			.context("Unable to create panels renderer")?;

		// Create egui
		let egui = Egui::new(window, &wgpu).context("Unable to create egui state")?;

		// Read all profiles
		let profiles = match util::parse_json_from_file("zsw_profiles.json") {
			Ok(profiles) => {
				log::info!("Loaded profiles {profiles:#?}");
				profiles
			},
			Err(err) => {
				log::info!("Unable to load profiles: {err:?}");
				vec![]
			},
		};

		Ok(Self {
			event_loop,
			inner: Inner {
				window,
				wgpu,
				paths_distributer,
				image_loader,
				panels,
				panels_renderer,
				egui,
				queued_settings_window_open_click: AtomicCell::new(None),
				settings_window_open: Mutex::new(false),
				// TODO: Copy existing panel, or surface
				new_panel_parameters: Mutex::new((
					Rect {
						pos:  Point2::new(0, 0),
						size: Vector2::new(0, 0),
					},
					15.0,
					0.85,
				)),
				_profiles: profiles,
			},
		})
	}

	/// Runs the app 'till completion
	pub fn run(mut self) -> Result<(), anyhow::Error> {
		// Start all threads and then wait in the main thread for events
		// TODO: Not ignore errors here, although given how `thread::scope` works
		//       it's somewhat hard to do so
		crossbeam::thread::scope(|s| {
			// Spawn the path distributer thread
			let _path_distributer = util::spawn_scoped(s, "Path distributer", || self.inner.paths_distributer.run())?;

			// Spawn all image loaders
			let loader_threads = thread::available_parallelism().map_or(1, NonZeroUsize::get);
			let _image_loaders =
				util::spawn_scoped_multiple(s, "Image loader", loader_threads, || || self.inner.image_loader.run())?;

			// Spawn the updater and renderer thread
			let _updater_thread = util::spawn_scoped(s, "Updater", || Self::run_updater(&self.inner))?;
			let _renderer_thread = util::spawn_scoped(s, "Renderer", || Self::run_renderer(&self.inner))?;

			// Run event loop in this thread until we quit
			let mut cursor_pos = PhysicalPosition::new(0.0, 0.0);
			self.event_loop.run_return(|event, _, control_flow| {
				Self::event_handler(&self.inner, event, control_flow, &mut cursor_pos);
			});

			anyhow::Ok(())
		})
		.expect("Unable to start all threads")
		.expect("Unable to run all threads 'till completion");

		Ok(())
	}

	/// Event handler
	fn event_handler(
		inner: &Inner, event: Event<!>, control_flow: &mut EventLoopControlFlow, cursor_pos: &mut PhysicalPosition<f64>,
	) {
		// Update egui
		inner.egui.platform().lock().handle_event(&event);

		// Set control for to wait for next event, since we're not doing
		// anything else on the main thread
		*control_flow = EventLoopControlFlow::Wait;

		// Then handle the event
		#[allow(clippy::single_match)] // We might add more in the future
		match event {
			Event::WindowEvent { event, .. } => match event {
				// if we should be closing, set the control flow to exit
				WindowEvent::CloseRequested | WindowEvent::Destroyed => {
					log::warn!("Received close request, closing window");
					*control_flow = EventLoopControlFlow::Exit;

					// Once we reach here, we can just exit, no need to
					// drop everything
					// TODO: Go through the drop in debug mode at least.
					std::process::exit(0);
				},

				// If we resized, queue a resize on wgpu
				WindowEvent::Resized(size) => inner.wgpu.resize(size),

				// On move, update the cursor position
				WindowEvent::CursorMoved { position, .. } => *cursor_pos = position,

				// If right clicked, queue a click
				WindowEvent::MouseInput {
					state: winit::event::ElementState::Pressed,
					button: winit::event::MouseButton::Right,
					..
				} => {
					inner.queued_settings_window_open_click.store(Some(*cursor_pos));
				},
				_ => (),
			},
			_ => (),
		}
	}

	/// Runs the updater
	fn run_updater(inner: &Inner) {
		// Duration we're sleep
		let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

		loop {
			// Render
			let (res, frame_duration) = crate::util::measure(|| Self::update(inner));
			match res {
				Ok(()) => log::trace!("Took {frame_duration:?} to render"),
				Err(err) => log::warn!("Unable to render: {err:?}"),
			};

			// Then sleep until next frame
			if let Some(duration) = sleep_duration.checked_sub(frame_duration) {
				thread::sleep(duration);
			}
		}
	}

	/// Updates
	fn update(inner: &Inner) -> Result<(), anyhow::Error> {
		let mut panels = inner.panels.lock();
		for panel in &mut *panels {
			if let Err(err) = panel.update(
				inner.wgpu.device(),
				inner.wgpu.queue(),
				inner.panels_renderer.uniforms_bind_group_layout(),
				inner.panels_renderer.texture_bind_group_layout(),
				&inner.image_loader,
			) {
				log::warn!("Unable to update panel: {err:?}");
			}
		}

		Ok(())
	}

	/// Runs the renderer
	fn run_renderer(inner: &Inner) {
		// Duration we're sleep
		let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

		loop {
			// Render
			let (res, frame_duration) = crate::util::measure(|| Self::render(inner));
			match res {
				Ok(()) => log::trace!("Took {frame_duration:?} to render"),
				Err(err) => log::warn!("Unable to render: {err:?}"),
			};

			// Then sleep until next frame
			if let Some(duration) = sleep_duration.checked_sub(frame_duration) {
				thread::sleep(duration);
			}
		}
	}

	/// Renders
	fn render(inner: &Inner) -> Result<(), anyhow::Error> {
		// Draw egui
		// TODO: When this is moved to it's own thread, regardless of issues with
		//       synchronizing the platform, we should synchronize the drawing to ensure
		//       we don't draw twice without displaying, as the first draw would never be
		//       visible to the user.
		let paint_jobs = inner
			.egui
			.draw(inner.window, |ctx, frame| {
				Self::draw_egui(inner, ctx, frame, inner.wgpu.surface_size())
			})
			.context("Unable to draw egui")?;

		inner.wgpu.render(|encoder, surface_view, surface_size| {
			// Render the panels
			let mut panels = inner.panels.lock();
			inner
				.panels_renderer
				.render(&mut *panels, encoder, surface_view, inner.wgpu.queue(), surface_size)
				.context("Unable to render panels")?;

			// Render egui
			#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
			let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
				physical_width:  surface_size.width,
				physical_height: surface_size.height,
				scale_factor:    inner.window.scale_factor() as f32,
			};
			let device = inner.wgpu.device();
			let queue = inner.wgpu.queue();
			let mut egui_render_pass = inner.egui.render_pass().lock();

			// TODO: Check if it's fine to get the platform here without synchronizing
			//       with the drawing.
			let egui_platform = inner.egui.platform().lock();
			egui_render_pass.update_texture(device, queue, &egui_platform.context().font_image());
			egui_render_pass.update_user_textures(device, queue);
			egui_render_pass.update_buffers(device, queue, &paint_jobs, &screen_descriptor);

			// Record all render passes.
			egui_render_pass
				.execute(encoder, surface_view, &paint_jobs, &screen_descriptor, None)
				.context("Unable to render egui")
		})
	}

	/// Draws egui app
	#[allow(unused_results)] // `egui` returns a response on every operation, but we don't use them
	fn draw_egui(
		inner: &Inner, ctx: &egui::CtxRef, _frame: &epi::Frame, surface_size: PhysicalSize<u32>,
	) -> Result<(), anyhow::Error> {
		let mut settings_window_open = inner.settings_window_open.lock();

		// Create the base settings window
		let mut settings_window = egui::Window::new("Settings");

		// If we have any queued click, summon the window there
		if let Some(cursor_pos) = inner.queued_settings_window_open_click.take() {
			// Adjust cursor pos to account for the scale factor
			let scale_factor = inner.window.scale_factor();
			let cursor_pos = cursor_pos.to_logical(scale_factor);

			// Then set the current position and that we're open
			settings_window = settings_window.current_pos(egui::pos2(cursor_pos.x, cursor_pos.y));
			*settings_window_open = true;
		}

		// Then render it
		settings_window.open(&mut *settings_window_open).show(ctx, |ui| {
			let mut panels = inner.panels.lock();
			for (idx, panel) in panels.iter_mut().enumerate() {
				ui.collapsing(format!("Panel {idx}"), |ui| {
					// TODO: Make a macro to make this more readable
					ui.horizontal(|ui| {
						ui.label("Geometry");
						self::draw_rect(ui, &mut panel.geometry, surface_size);
					});
					ui.horizontal(|ui| {
						ui.label("Progress");
						egui::Slider::new(&mut panel.progress, 0.0..=0.99).ui(ui);
					});
					ui.horizontal(|ui| {
						ui.label("Fade point");
						egui::Slider::new(&mut panel.fade_point, 0.5..=1.0).ui(ui);
					});
					ui.horizontal(|ui| {
						ui.label("Duration");
						let mut seconds = panel.image_duration.as_secs_f32();
						egui::Slider::new(&mut seconds, 0.5..=180.0).ui(ui);
						panel.image_duration = Duration::from_secs_f32(seconds);
					});

					// On skip, skip the current panel
					// TODO: Do this properly
					ui.horizontal(|ui| {
						ui.label("Skip");
						if ui.button("🔄").clicked() {
							//panel.state = PanelState::Empty;
							panel.progress = 1.0;
						}
					});
				});
			}
			ui.collapsing("Add panel", |ui| {
				let mut new_panel_parameters = inner.new_panel_parameters.lock();
				let (geometry, image_duration, fade_point) = &mut *new_panel_parameters;

				ui.horizontal(|ui| {
					ui.label("Geometry");
					self::draw_rect(ui, geometry, surface_size);
				});

				ui.horizontal(|ui| {
					ui.label("Fade point");
					egui::Slider::new(fade_point, 0.5..=1.0).ui(ui);
				});

				ui.horizontal(|ui| {
					ui.label("Duration");
					egui::Slider::new(image_duration, 0.5..=180.0).ui(ui);
				});

				if ui.button("Add").clicked() {
					panels.push(Panel::new(
						*geometry,
						PanelState::Empty,
						Duration::from_secs_f32(*image_duration),
						*fade_point,
					));
				}
			});
			mem::drop(panels);

			ui.horizontal(|ui| {
				let cur_root_path = inner.paths_distributer.root_path();

				ui.label("Root path");
				ui.label(cur_root_path.display().to_string());
				if ui.button("📁").clicked() {
					let file_dialog = native_dialog::FileDialog::new()
						.set_location(&*cur_root_path)
						.show_open_single_dir();
					match file_dialog {
						Ok(file_dialog) => {
							if let Some(root_path) = file_dialog {
								// Set the root path
								inner.paths_distributer.set_root_path(root_path);

								// TODO: Reset all existing images and paths loaded from the
								//       old path distributer, maybe?
							}
						},
						Err(err) => log::warn!("Unable to ask user for new root directory: {err:?}"),
					}
				}
			});
		});

		Ok(())
	}
}

fn draw_rect(ui: &mut egui::Ui, geometry: &mut Rect<u32>, max_size: PhysicalSize<u32>) -> egui::Response {
	// Calculate the limits
	// TODO: If two values are changed at the same time, during 1 frame it's
	//       possible for the values to be out of range.
	let max_width = max_size.width;
	let max_height = max_size.height;
	let max_x = max_size.width.saturating_sub(geometry.size.x);
	let max_y = max_size.height.saturating_sub(geometry.size.y);

	// new_panel_parameters

	let mut response = egui::DragValue::new(&mut geometry.size.x)
		.clamp_range(0..=max_width)
		.speed(10)
		.ui(ui);
	response |= ui.label("x");
	response |= egui::DragValue::new(&mut geometry.size.y)
		.clamp_range(0..=max_height)
		.speed(10)
		.ui(ui);
	response |= ui.label("+");
	response |= egui::DragValue::new(&mut geometry.pos.x)
		.clamp_range(0..=max_x)
		.speed(10)
		.ui(ui);
	response |= ui.label("+");
	response |= egui::DragValue::new(&mut geometry.pos.y)
		.clamp_range(0..=max_y)
		.speed(10)
		.ui(ui);

	response
}

/// Creates the window, as well as the associated event loop
fn create_window(args: &Args) -> Result<(EventLoop<!>, &'static Window), anyhow::Error> {
	// Build the window
	// TODO: Not leak the window
	let event_loop = EventLoop::with_user_event();
	log::debug!("Creating window (geometry: {:?})", args.window_geometry);
	let window = WindowBuilder::new()
		.with_position(PhysicalPosition {
			x: args.window_geometry.pos[0],
			y: args.window_geometry.pos[1],
		})
		.with_inner_size(PhysicalSize {
			width:  args.window_geometry.size[0],
			height: args.window_geometry.size[1],
		})
		.with_decorations(false)
		.with_x11_window_type(vec![XWindowType::Desktop])
		.build(&event_loop)
		.context("Unable to build window")?;
	let window = Box::leak(Box::new(window));

	// Set the window as always below
	// Note: Required so it doesn't hide itself if the desktop is clicked on
	// SAFETY: TODO
	unsafe {
		self::set_display_always_below(window);
	}

	Ok((event_loop, window))
}

/// Sets the display as always below
///
/// # Safety
/// TODO
#[allow(clippy::expect_used)] // TODO: Refactor all of this
unsafe fn set_display_always_below(window: &Window) {
	// Get the xlib display and window
	let display = window.xlib_display().expect("No `X` display found").cast();
	let window = window.xlib_window().expect("No `X` window found");

	// Flush the existing `XMapRaised`
	assert_eq!(unsafe { xlib::XFlush(display) }, 1);
	thread::sleep(Duration::from_millis(100));

	// Unmap the window temporarily
	assert_eq!(unsafe { xlib::XUnmapWindow(display, window) }, 1);
	assert_eq!(unsafe { xlib::XFlush(display) }, 1);
	thread::sleep(Duration::from_millis(100));

	// Add the always below hint to the window manager
	{
		let property = unsafe { xlib::XInternAtom(display, b"_NET_WM_STATE\0".as_ptr().cast(), 0) };
		let value = unsafe { xlib::XInternAtom(display, b"_NET_WM_STATE_BELOW\0".as_ptr().cast(), 0) };
		let res = unsafe {
			xlib::XChangeProperty(
				display,
				window,
				property,
				xlib::XA_ATOM,
				32,
				xlib::PropModeAppend,
				std::ptr::addr_of!(value).cast(),
				1,
			)
		};
		assert_eq!(res, 1, "Unable to change window property");
	}

	// Then remap it
	assert_eq!(unsafe { xlib::XMapRaised(display, window) }, 1);
	assert_eq!(unsafe { xlib::XFlush(display) }, 1);
}
