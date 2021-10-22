//! Zenithsiz's scrolling wallpaper

// Features
#![feature(never_type, format_args_capture, available_parallelism, drain_filter, array_zip)]

// Modules
mod args;
mod gl_image;
mod image_loader;
mod image_uvs;
mod rect;
mod vertex;

// Exports
pub use gl_image::GlImage;
pub use image_loader::{ImageBuffer, ImageLoader};
pub use image_uvs::ImageUvs;
pub use rect::Rect;
pub use vertex::Vertex;

// Imports
use anyhow::Context;
use cgmath::{EuclideanSpace, Matrix4, Point2, Vector3};
use glium::{
	glutin::{
		self,
		event::{Event, StartCause, WindowEvent},
		event_loop::ControlFlow as GlutinControlFlow,
		platform::unix::{EventLoopExtUnix, WindowBuilderExtUnix, WindowExtUnix, XWindowType},
	},
	index::PrimitiveType,
	program::ProgramCreationInput,
	Surface,
};
use std::{
	mem,
	time::{Duration, Instant},
};
use x11::xlib;

#[allow(clippy::too_many_lines)] // TODO: Refactor
fn main() -> Result<(), anyhow::Error> {
	// Initialize logger
	simplelog::TermLogger::init(
		log::LevelFilter::Debug,
		simplelog::Config::default(),
		simplelog::TerminalMode::Stderr,
		simplelog::ColorChoice::Auto,
	)
	.context("Unable to initialize logger")?;

	// Get arguments
	let args = args::get().context("Unable to retrieve arguments")?;
	log::debug!("Found arguments {args:?}");

	// Create the event loop and build the display.
	let event_loop =
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

	// Create the loader and start loading images
	let mut image_loader = ImageLoader::new(args.images_dir, args.image_backlog, [
		args.window_geometry.size.x,
		args.window_geometry.size.y,
	])
		.context("Unable to create image loader")?;

	// Get the window size
	let window_size = display.gl_window().window().inner_size();
	let window_size = [window_size.width, window_size.height];

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
	let mut geometry_states = args
		.image_geometries
		.iter()
		.map(|&geometry| {
			let mut get_image = || {
				GlImage::new(
					&display,
					image_loader.next_image().context("Unable to get next image")?,
					[geometry.size.x, geometry.size.y],
				)
				.context("Unable to create image")
			};
			Ok(GeometryState {
				geometry,
				cur_image: get_image()?,
				next_image: get_image()?,
				progress: rand::random(),
				next_image_is_loaded: false,
			})
		})
		.collect::<Result<Vec<_>, anyhow::Error>>()
		.context("Unable to load images for all geometries")?;

	// Current cursor position
	let mut cursor_pos = Point2::origin();

	event_loop.run(move |event, _, control_flow| match event {
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
			for geometry_state in &mut geometry_states {
				self::draw_update(
					&mut target,
					geometry_state,
					args.image_duration,
					args.fade_point,
					&indices,
					&program,
					&display,
					&mut image_loader,
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
	});
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
	indices: &glium::IndexBuffer<u32>, program: &glium::Program, display: &glium::Display,
	image_loader: &mut ImageLoader, window_size: [u32; 2], cursor_pos: Point2<f32>,
) {
	if let Err(err) = self::draw(target, geometry_state, fade, indices, program, window_size, cursor_pos) {
		// Note: We just want to ensure we don't get a panic by dropping an unwrapped target
		let _ = target.set_finish();
		log::warn!("Unable to draw: {err:?}");
	}

	if let Err(err) = self::update(geometry_state, duration, fade, display, image_loader) {
		log::warn!("Unable to update: {err:?}");
	}
}

/// Updates
fn update(
	geometry_state: &mut GeometryState, duration: Duration, fade: f32, display: &glium::Display,
	image_loader: &mut ImageLoader,
) -> Result<(), anyhow::Error> {
	// Increase the progress
	geometry_state.progress += (1.0 / 60.0) / duration.as_secs_f32();

	// If the next image isn't loaded, try to load it
	if !geometry_state.next_image_is_loaded {
		// If our progress is >= fade start, then we have to force wait for the image.
		let force_wait = geometry_state.progress >= fade;

		if force_wait {
			log::warn!("Next image hasn't arrived yet at the end of current image, waiting for it");
		}

		// Then try to load it
		geometry_state.next_image_is_loaded ^= geometry_state
			.next_image
			.try_update(display, image_loader, force_wait)
			.context("Unable to update image")?;

		// If we force waited but the next image isn't loaded, return Err
		if force_wait && !geometry_state.next_image_is_loaded {
			return Err(anyhow::anyhow!("Unable to load next image even while force-waiting"));
		}
	}

	// If we reached the end, swap the next to current and try to load the next
	if geometry_state.progress >= 1.0 {
		// Reset the progress to where we where during the fade
		geometry_state.progress = 1.0 - fade;

		// Swap the images
		mem::swap(&mut geometry_state.cur_image, &mut geometry_state.next_image);
		geometry_state.next_image_is_loaded = false;

		// And try to update the next image
		geometry_state.next_image_is_loaded ^= geometry_state
			.next_image
			.try_update(display, image_loader, false)
			.context("Unable to update image")?;
	}


	Ok(())
}

/// Draws
#[allow(clippy::cast_precision_loss)] // Image and window sizes are far below 2^23
fn draw(
	target: &mut glium::Frame, geometry_state: &mut GeometryState, fade: f32, indices: &glium::IndexBuffer<u32>,
	program: &glium::Program, window_size: [u32; 2], cursor_pos: Point2<f32>,
) -> Result<(), anyhow::Error> {
	// Calculate the base alpha and progress to apply to the images
	let (base_alpha, next_progress) = match geometry_state.progress {
		f if f >= fade => (
			(geometry_state.progress - fade) / (1.0 - fade),
			geometry_state.progress - fade,
		),
		_ => (0.0, 0.0),
	};

	// Then draw
	for (image, alpha, progress) in [
		(&mut geometry_state.cur_image, 1.0 - base_alpha, geometry_state.progress),
		(&mut geometry_state.next_image, base_alpha, next_progress),
	] {
		// If alpha is 0, don't render
		if alpha == 0.0 {
			continue;
		}

		// Calculate the matrix for the geometry
		let x_scale = geometry_state.geometry.size[0] as f32 / window_size[0] as f32;
		let y_scale = geometry_state.geometry.size[1] as f32 / window_size[1] as f32;

		let x_offset = geometry_state.geometry.pos[0] as f32 / window_size[0] as f32;
		let y_offset = geometry_state.geometry.pos[1] as f32 / window_size[1] as f32;

		let mat = Matrix4::from_translation(Vector3::new(
			-1.0 + x_scale + 2.0 * x_offset,
			1.0 - y_scale - 2.0 * y_offset,
			0.0,
		)) * Matrix4::from_nonuniform_scale(x_scale, y_scale, 1.0);

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
struct GeometryState {
	/// Geometry
	geometry: Rect<u32>,

	/// Current image
	cur_image: GlImage,

	/// Next image
	next_image: GlImage,

	/// Progress
	progress: f32,

	/// If the next image is loaded
	next_image_is_loaded: bool,
}
