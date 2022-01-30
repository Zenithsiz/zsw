//! App

// Lints
#![allow(clippy::too_many_arguments)] // We need to share a lot of state and we can't couple it together in most cases

// Modules
mod event_handler;
mod renderer;

// Imports
use self::{event_handler::EventHandler, renderer::Renderer};
use crate::{paths, util, Args, Egui, ImageLoader, Panel, PanelState, PanelsProfile, PanelsRenderer, Wgpu};
use anyhow::Context;
use crossbeam::atomic::AtomicCell;
use parking_lot::Mutex;
use std::{num::NonZeroUsize, thread, time::Duration};
use winit::{
	dpi::{PhysicalPosition, PhysicalSize},
	event_loop::EventLoop,
	platform::{
		run_return::EventLoopExtRunReturn,
		unix::{WindowBuilderExtUnix, WindowExtUnix, XWindowType},
	},
	window::{Window, WindowBuilder},
};
use x11::xlib;

/// Runs the application
pub fn run(args: &Args) -> Result<(), anyhow::Error> {
	// Build the window
	let (mut event_loop, window) = self::create_window(args)?;

	// Create the wgpu interface
	let wgpu = Wgpu::new(&window).context("Unable to create renderer")?;

	// Create the paths channel
	let (paths_distributer, paths_rx) = paths::new(args.images_dir.clone());

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
		.context("Unable to create panels renderer")?;

	// Create egui
	let egui = Egui::new(&window, &wgpu).context("Unable to create egui state")?;

	// Read all profiles
	let _profiles: Vec<PanelsProfile> = match util::parse_json_from_file("zsw_profiles.json") {
		Ok(profiles) => {
			log::info!("Loaded profiles {profiles:#?}");
			profiles
		},
		Err(err) => {
			log::info!("Unable to load profiles: {err:?}");
			vec![]
		},
	};

	let queued_settings_window_open_click = AtomicCell::new(None);

	// Create the event handler
	let mut event_handler = EventHandler::new(&wgpu, &egui, &queued_settings_window_open_click);

	// Create the renderer
	let mut renderer = Renderer::new(
		&window,
		&wgpu,
		&paths_distributer,
		&image_loader,
		&panels_renderer,
		&panels,
		&egui,
		&queued_settings_window_open_click,
	);

	// Start all threads and then wait in the main thread for events
	// TODO: Not ignore errors here, although given how `thread::scope` works
	//       it's somewhat hard to do so
	crossbeam::thread::scope(|s| {
		// Spawn the path distributer thread
		let _path_distributer = util::spawn_scoped(s, "Path distributer", || paths_distributer.run())?;

		// Spawn all image loaders
		let loader_threads = thread::available_parallelism().map_or(1, NonZeroUsize::get);
		let _image_loaders = util::spawn_scoped_multiple(s, "Image loader", loader_threads, || || image_loader.run())?;

		// Spawn the renderer thread
		let _renderer_thread = util::spawn_scoped(s, "Renderer", || renderer.run())?;

		// Run event loop in this thread until we quit
		event_loop.run_return(|event, _, control_flow| {
			event_handler.handle_event(event, control_flow);
		});

		anyhow::Ok(())
	})
	.expect("Unable to start all threads")
	.expect("Unable to run all threads 'till completion");

	Ok(())
}

/// Creates the window, as well as the associated event loop
fn create_window(args: &Args) -> Result<(EventLoop<!>, Window), anyhow::Error> {
	// Build the window
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

	// Set the window as always below
	// Note: Required so it doesn't hide itself if the desktop is clicked on
	// SAFETY: TODO
	unsafe {
		self::set_display_always_below(&window);
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
