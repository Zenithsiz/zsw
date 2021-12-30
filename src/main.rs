//! Zenithsiz's scrolling wallpaper

// Features
#![feature(
	never_type,
	available_parallelism,
	drain_filter,
	array_zip,
	control_flow_enum,
	unwrap_infallible,
	derive_default_enum,
	decl_macro
)]

// Modules
mod args;
mod gl_image;
mod img;
mod path_loader;
mod rect;
mod sync;
mod util;
mod vertex;


// Imports
use crate::{
	gl_image::GlImage,
	img::{ImageLoader, ImageLoaderArgs, ImageUvs},
	path_loader::PathLoader,
	rect::Rect,
	vertex::Vertex,
};
use anyhow::Context;
use cgmath::{EuclideanSpace, Matrix4, Point2, Vector3};
use glium::{
	glutin::{
		self,
		event::{Event, StartCause, WindowEvent},
		event_loop::{ControlFlow as GlutinControlFlow, EventLoopWindowTarget},
		platform::{
			run_return::EventLoopExtRunReturn,
			unix::{EventLoopExtUnix, WindowBuilderExtUnix, WindowExtUnix, XWindowType},
		},
	},
	index::PrimitiveType,
	program::ProgramCreationInput,
	Surface,
};
use std::{
	fs,
	time::{Duration, Instant},
};
use x11::xlib;

#[allow(clippy::too_many_lines)] // TODO: Refactor
fn main() -> Result<(), anyhow::Error> {
	// Initialize logger
	match self::init_log() {
		Ok(()) => log::debug!("Initialized logging"),
		Err(err) => eprintln!("Unable to initialize logger: {err:?}"),
	}

	// Get arguments
	let args = args::get().context("Unable to retrieve arguments")?;
	log::debug!("Found arguments {args:?}");

	// Create the event loop and build the display.
	log::debug!("Building the window");
	let mut event_loop =
		glium::glutin::event_loop::EventLoop::<!>::new_x11().context("Unable to create an x11 event loop")?;
	let window_builder = glutin::window::WindowBuilder::new()
		.with_position(glutin::dpi::PhysicalPosition {
			x: args.window_geometry.pos[0],
			y: args.window_geometry.pos[1],
		})
		.with_inner_size(glutin::dpi::PhysicalSize {
			width:  args.window_geometry.size[0],
			height: args.window_geometry.size[1],
		})
		.with_x11_window_type(vec![XWindowType::Desktop]);
	let context_builder = glutin::ContextBuilder::new();
	let display =
		glium::Display::new(window_builder, context_builder, &event_loop).context("Unable to create glutin display")?;

	// Set the window as always below
	// Note: Required so it doesn't hide itself if the desktop is clicked on
	// SAFETY: TODO
	unsafe {
		self::set_display_always_below(&display);
	}

	// Create the path loader
	log::debug!("Starting the path loader");
	let path_loader = PathLoader::new(args.images_dir.clone()).context("Unable to create path loader")?;

	// Create the loader
	log::debug!("Starting the image loader");
	let image_loader_args = ImageLoaderArgs {
		loader_threads:  args.loader_threads,
		upscale:         args.upscale,
		upscale_waifu2x: args.upscale_waifu2x,
		downscale:       args.downscale,
	};
	let image_loader = ImageLoader::new(&path_loader, image_loader_args).context("Unable to create image loader")?;

	// Create the indices buffer
	const INDICES: [u32; 6] = [0, 1, 3, 0, 3, 2];
	let indices = glium::IndexBuffer::<u32>::new(&display, PrimitiveType::TrianglesList, &INDICES)
		.context("Unable to create index buffer")?;

	// Create the program
	let program = {
		glium::Program::new(&display, ProgramCreationInput::SourceCode {
			vertex_shader:                  include_str!("shader/vertex.glsl"),
			fragment_shader:                include_str!("shader/frag.glsl"),
			geometry_shader:                None,
			tessellation_control_shader:    None,
			tessellation_evaluation_shader: None,
			transform_feedback_varyings:    None,
			outputs_srgb:                   true,
			uses_point_size:                false,
		})
	}
	.context("Unable to build program")?;

	// All geometry states
	log::debug!("Creating all geometries");
	let mut geometry_states = args
		.image_geometries
		.iter()
		.map(|&geometry| GeometryState {
			geometry,
			images: GeometryImageState::Empty,
			progress: rand::random(),
		})
		.collect::<Vec<_>>();


	// Get the event handler, and then run until it returns
	log::debug!("Entering event handler");
	let event_handler = self::event_handler(display, &mut geometry_states, &args, indices, program, &image_loader);
	event_loop.run_return(event_handler);

	Ok(())
}

fn event_handler<'a>(
	display: glium::Display, geometry_states: &'a mut [GeometryState], args: &'a args::Args,
	indices: glium::IndexBuffer<u32>, program: glium::Program, image_loader: &'a ImageLoader,
) -> impl 'a + FnMut(Event<'_, !>, &EventLoopWindowTarget<!>, &mut glutin::event_loop::ControlFlow) {
	// Current cursor position
	let mut cursor_pos = Point2::origin();

	// Get the window size
	let window_size = display.gl_window().window().inner_size();
	let window_size = [window_size.width, window_size.height];

	move |event, _, control_flow| match event {
		Event::WindowEvent { event, .. } => match event {
			// If we got a close request, exit and return
			WindowEvent::CloseRequested | WindowEvent::Destroyed => {
				*control_flow = GlutinControlFlow::Exit;
			},

			#[allow(clippy::cast_possible_truncation)] // We're fine with truncating the values
			WindowEvent::CursorMoved { position, .. } => cursor_pos = Point2::new(position.x as f32, position.y as f32),

			_ => (),
		},
		// If it's time to draw, draw
		Event::NewEvents(StartCause::ResumeTimeReached { .. } | StartCause::Init) => {
			// Set the next frame to 1/60th of a second from now
			*control_flow = GlutinControlFlow::WaitUntil(Instant::now() + Duration::from_secs(1) / 60);

			// Draw
			let mut target = display.draw();

			// Clear the screen
			target.clear_color(0.0, 0.0, 0.0, 1.0);

			// Draw each image geometry
			for geometry_state in geometry_states.iter_mut() {
				self::draw_update(
					&mut target,
					geometry_state,
					args.image_duration,
					args.fade_point,
					&indices,
					&program,
					&display,
					image_loader,
					window_size,
					cursor_pos,
				);
			}

			// Finish drawing
			if let Err(err) = target.finish() {
				log::error!("Unable to swap buffers: {err}");
			}
		},
		_ => (),
	}
}

/// Sets the display as always below
///
/// # Safety
/// TODO
// TODO: Do this through `glutin`, this is way too hacky
#[allow(clippy::expect_used)] // TODO: Refactor all of this
#[deny(unsafe_op_in_unsafe_fn)] // Necessary to prevent warnings during non-clippy operations
unsafe fn set_display_always_below(display: &glium::Display) {
	// Get the xlib display and window
	let gl_window = display.gl_window();
	let window = gl_window.window();
	let display = window.xlib_display().expect("No `X` display found").cast();
	let window = window.xlib_window().expect("No `X` window found");

	// Flush the existing `XMapRaised`
	unsafe { xlib::XFlush(display) };
	std::thread::sleep(Duration::from_millis(100));

	// Unmap the window temporarily
	unsafe { xlib::XUnmapWindow(display, window) };
	unsafe { xlib::XFlush(display) };
	std::thread::sleep(Duration::from_millis(100));

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
				(&value as *const u64).cast(),
				1,
			)
		};
		assert_eq!(res, 1, "Unable to change window property");
	}

	// Then remap it
	unsafe { xlib::XMapRaised(display, window) };
	unsafe { xlib::XFlush(display) };
}


/// Draws and updates
#[allow(clippy::too_many_arguments)] // TODO: Refactor, closure doesn't work, though
fn draw_update(
	target: &mut glium::Frame, geometry_state: &mut GeometryState, duration: Duration, fade: f32,
	indices: &glium::IndexBuffer<u32>, program: &glium::Program, display: &glium::Display, image_loader: &ImageLoader,
	window_size: [u32; 2], cursor_pos: Point2<f32>,
) {
	if let Err(err) = self::draw(target, geometry_state, fade, indices, program, window_size, cursor_pos) {
		log::warn!("Unable to draw: {err:?}");
	}

	if let Err(err) = self::update(geometry_state, duration, fade, display, image_loader) {
		log::warn!("Unable to update: {err:?}");
	}
}

/// Updates
fn update(
	geometry_state: &mut GeometryState, duration: Duration, fade: f32, display: &glium::Display,
	image_loader: &ImageLoader,
) -> Result<(), anyhow::Error> {
	// Increase the progress
	geometry_state.progress += (1.0 / 60.0) / duration.as_secs_f32();

	// If we need to force wait for the next image
	let force_wait = geometry_state.progress >= fade;

	// If we finished the current image
	let finished = geometry_state.progress >= 1.0;

	// Check the image state
	let geometry = geometry_state.geometry;
	geometry_state.images = match std::mem::replace(&mut geometry_state.images, GeometryImageState::Empty) {
		// Regardless of progress, we must wait for the first image
		GeometryImageState::Empty => {
			let image = GlImage::new(display, image_loader, geometry.size).context("Unable to create image")?;
			GeometryImageState::PrimaryOnly(image)
		},

		// If we only have the primary, load a new image if we need to force wait
		// TODO: Try to load it earlier
		GeometryImageState::PrimaryOnly(cur) if force_wait => {
			let next = GlImage::new(display, image_loader, geometry.size).context("Unable to create image")?;
			GeometryImageState::Both { cur, next }
		},
		state @ GeometryImageState::PrimaryOnly(_) => state,

		// If we got both, change to swapped if we've reached the end, else don't do anything
		GeometryImageState::Both { cur, next } if finished => {
			geometry_state.progress = 1.0 - fade;
			GeometryImageState::Swapped { cur: next, prev: cur }
		},
		state @ GeometryImageState::Both { .. } => state,

		// If they've been swapped, try to update the previous
		GeometryImageState::Swapped { mut prev, cur } => match prev
			.try_update(display, image_loader, force_wait)
			.context("Unable to get next image")?
		{
			true => GeometryImageState::Both { cur, next: prev },
			false => GeometryImageState::Swapped { prev, cur },
		},
	};

	Ok(())
}

/// Draws
#[allow(clippy::cast_precision_loss)] // Image and window sizes are far below 2^23
fn draw(
	target: &mut glium::Frame, geometry_state: &mut GeometryState, fade: f32, indices: &glium::IndexBuffer<u32>,
	program: &glium::Program, window_size: [u32; 2], _cursor_pos: Point2<f32>,
) -> Result<(), anyhow::Error> {
	// Calculate the base alpha and progress to apply to the images
	let cur_progress = geometry_state.progress;
	let (base_alpha, next_progress) = match cur_progress {
		f if f >= fade => ((cur_progress - fade) / (1.0 - fade), cur_progress - fade),
		_ => (0.0, 0.0),
	};

	let (cur, next) = match &mut geometry_state.images {
		GeometryImageState::Empty => (None, None),
		GeometryImageState::PrimaryOnly(cur) | GeometryImageState::Swapped { cur, .. } => {
			(Some((cur, 1.0, cur_progress)), None)
		},
		GeometryImageState::Both { cur, next } => (
			Some((cur, 1.0 - base_alpha, cur_progress)),
			Some((next, base_alpha, next_progress)),
		),
	};

	// Then draw
	let geometry = geometry_state.geometry;
	for (image, alpha, progress) in [cur, next].into_iter().flatten() {
		// Calculate the matrix for the geometry
		let x_scale = geometry.size[0] as f32 / window_size[0] as f32;
		let y_scale = geometry.size[1] as f32 / window_size[1] as f32;

		let x_offset = geometry.pos[0] as f32 / window_size[0] as f32;
		let y_offset = geometry.pos[1] as f32 / window_size[1] as f32;

		let mat = Matrix4::from_translation(Vector3::new(
			-1.0 + x_scale + 2.0 * x_offset,
			1.0 - y_scale - 2.0 * y_offset,
			0.0,
		)) * Matrix4::from_nonuniform_scale(x_scale, -y_scale, 1.0);

		// Setup the uniforms with all the data
		let sampler = image.texture.sampled();
		let tex_offset = image.uvs.offset(progress);
		let uniforms = glium::uniform! {
			mat: *<_ as AsRef<[[f32; 4]; 4]>>::as_ref(&mat),
			tex_sampler: sampler,
			tex_offset: tex_offset,
			alpha: alpha,
		};

		// And draw
		let draw_parameters = glium::DrawParameters {
			blend: glium::Blend::alpha_blending(),
			..glium::DrawParameters::default()
		};
		target
			.draw(&image.vertex_buffer, indices, program, &uniforms, &draw_parameters)
			.context("Unable to draw")?;
	}

	Ok(())
}

/// Geometry state
#[derive(Debug)]
struct GeometryState {
	/// Geometry
	geometry: Rect<u32>,

	/// Images
	images: GeometryImageState,

	/// Progress
	progress: f32,
}

/// Image state of the geometry
#[derive(Debug)]
enum GeometryImageState {
	/// Empty
	///
	/// This means that no images have been assigned to this geometry yet.
	Empty,

	/// Primary only
	///
	/// The primary image is loaded. The back image is still not available
	PrimaryOnly(GlImage),

	/// Both
	///
	/// Both images are loaded to be faded in between
	Both {
		/// Current image
		cur: GlImage,

		/// Next
		next: GlImage,
	},

	/// Swapped
	///
	/// Front and back images have been swapped, and the next image needs
	/// to be loaded
	Swapped {
		/// Previous image
		prev: GlImage,

		/// Current image
		cur: GlImage,
	},
}

/// Initializes the logging
fn init_log() -> Result<(), anyhow::Error> {
	/// Creates the file logger
	fn file_logger() -> Result<Box<simplelog::WriteLogger<fs::File>>, anyhow::Error> {
		let file = fs::File::create("latest.log").context("Unable to create file `latest.log`")?;
		Ok(simplelog::WriteLogger::new(
			log::LevelFilter::Trace,
			simplelog::Config::default(),
			file,
		))
	}

	// All loggers
	let mut loggers = Vec::with_capacity(2);

	// Create the term logger
	let term_logger = simplelog::TermLogger::new(
		log::LevelFilter::Info,
		simplelog::Config::default(),
		simplelog::TerminalMode::Stderr,
		simplelog::ColorChoice::Auto,
	);
	loggers.push(term_logger as Box<_>);

	// Then try to create the file logger
	let file_logger_res = file_logger().map(|file_logger| loggers.push(file_logger as _));

	// Finally initialize them all
	simplelog::CombinedLogger::init(loggers).context("Unable to initialize loggers")?;

	// Then check if we got any errors
	if let Err(err) = file_logger_res {
		log::warn!("Unable to initialize file logger: {err:?}");
	}

	Ok(())
}
