//! Application state

// Modules
mod geometry;

// Imports
use self::geometry::{Geometry, GeometryImage};
use crate::{
	args::Args,
	renderer::{Renderer, Vertex},
};
use anyhow::Context;
use cgmath::{Matrix4, Vector3};
use crossbeam::thread;
use std::time::{Duration, Instant};
use wgpu::util::DeviceExt;
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

/// Application state
pub struct App {
	/// Arguments
	args: Args,

	/// Event loop
	event_loop: EventLoop<!>,

	/// Window
	window: &'static Window,

	/// Renderer
	renderer: Renderer,

	/// Geometries
	geometries: Vec<Geometry>,

	/// Render pipeline for all geometries
	render_pipeline: wgpu::RenderPipeline,

	/// Index buffer for all geometries
	///
	/// Since we're just rendering rectangles, the indices
	/// buffer is shared for all geometries for now.
	indices: wgpu::Buffer,

	/// Uniform buffer
	uniform_buffer: wgpu::Buffer,
}

impl App {
	/// Creates a new app
	#[allow(clippy::future_not_send)] // Unfortunately we can't do much about it, we must build the window in the main thread
	#[allow(clippy::too_many_lines)] // TODO:
	pub async fn new(args: Args) -> Result<Self, anyhow::Error> {
		// Build the window
		// TODO: Not leak the window
		let event_loop = EventLoop::with_user_event();
		let window = WindowBuilder::new()
			.with_position(PhysicalPosition {
				x: args.window_geometry.pos[0],
				y: args.window_geometry.pos[1],
			})
			.with_inner_size(PhysicalSize {
				width:  args.window_geometry.size[0],
				height: args.window_geometry.size[1],
			})
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

		// Create the renderer
		let renderer = Renderer::new(window).await.context("Unable to create renderer")?;

		// Create all geometries
		let geometries = args
			.image_geometries
			.iter()
			.map(|&geometry| Geometry {
				geometry,
				image: GeometryImage::Empty,
				progress: rand::random(),
			})
			.collect::<Vec<_>>();

		// Create the index buffer
		const INDICES: [u32; 6] = [0, 1, 3, 0, 3, 2];
		let index_buffer_descriptor = wgpu::util::BufferInitDescriptor {
			label:    Some("Index buffer"),
			contents: bytemuck::cast_slice(&INDICES),
			usage:    wgpu::BufferUsages::INDEX,
		};
		let indices = renderer.device().create_buffer_init(&index_buffer_descriptor);

		// Load the shader
		let shader_descriptor = wgpu::ShaderModuleDescriptor {
			label:  Some("Shader"),
			source: wgpu::ShaderSource::Wgsl(include_str!("renderer/shader.wgsl").into()),
		};
		let shader = renderer.device().create_shader_module(&shader_descriptor);

		// Create the pipeline layout
		let render_pipeline_layout_descriptor = wgpu::PipelineLayoutDescriptor {
			label:                Some("Render pipeline layout"),
			bind_group_layouts:   &[],
			push_constant_ranges: &[],
		};
		let render_pipeline_layout = renderer
			.device()
			.create_pipeline_layout(&render_pipeline_layout_descriptor);

		// Create the render pipeline
		let color_targets = [wgpu::ColorTargetState {
			format:     renderer.config().format,
			blend:      Some(wgpu::BlendState::REPLACE),
			write_mask: wgpu::ColorWrites::ALL,
		}];
		let render_pipeline_descriptor = wgpu::RenderPipelineDescriptor {
			label:         Some("Render pipeline"),
			layout:        Some(&render_pipeline_layout),
			vertex:        wgpu::VertexState {
				module:      &shader,
				entry_point: "vs_main",
				buffers:     &[Vertex::buffer_layout()],
			},
			primitive:     wgpu::PrimitiveState {
				topology:           wgpu::PrimitiveTopology::TriangleList,
				strip_index_format: None,
				front_face:         wgpu::FrontFace::Ccw,
				cull_mode:          Some(wgpu::Face::Back),
				unclipped_depth:    false,
				polygon_mode:       wgpu::PolygonMode::Fill,
				conservative:       false,
			},
			depth_stencil: None,
			multisample:   wgpu::MultisampleState {
				count: 1,
				mask: u64::MAX,
				alpha_to_coverage_enabled: false,
			},
			fragment:      Some(wgpu::FragmentState {
				module:      &shader,
				entry_point: "fs_main",
				targets:     &color_targets,
			}),
			multiview:     None,
		};
		let render_pipeline = renderer.device().create_render_pipeline(&render_pipeline_descriptor);

		let uniforms = [Uniforms::default()];
		let uniforms_descriptor = wgpu::util::BufferInitDescriptor {
			label:    Some("Camera Buffer"),
			contents: bytemuck::cast_slice(&uniforms),
			usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		};
		let uniforms = renderer.device().create_buffer_init(&uniforms_descriptor);

		let bind_group_layout_descriptor = wgpu::BindGroupLayoutDescriptor {
			entries: &[
				wgpu::BindGroupLayoutEntry {
					binding:    0,
					visibility: wgpu::ShaderStages::FRAGMENT,
					ty:         wgpu::BindingType::Texture {
						multisampled:   false,
						view_dimension: wgpu::TextureViewDimension::D2,
						sample_type:    wgpu::TextureSampleType::Float { filterable: true },
					},
					count:      None,
				},
				wgpu::BindGroupLayoutEntry {
					binding:    1,
					visibility: wgpu::ShaderStages::FRAGMENT,
					ty:         wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
					count:      None,
				},
			],
			label:   None,
		};
		let texture_bind_group_layout = renderer
			.device()
			.create_bind_group_layout(&bind_group_layout_descriptor);

		let texture_view = image.texture.create_view(&wgpu::TextureViewDescriptor::default());
		let sampler_descriptor = wgpu::SamplerDescriptor {
			address_mode_u: wgpu::AddressMode::ClampToEdge,
			address_mode_v: wgpu::AddressMode::ClampToEdge,
			address_mode_w: wgpu::AddressMode::ClampToEdge,
			mag_filter: wgpu::FilterMode::Linear,
			min_filter: wgpu::FilterMode::Nearest,
			mipmap_filter: wgpu::FilterMode::Nearest,
			..Default::default()
		};
		let sampler = device.create_sampler(&sampler_descriptor);

		let bind_group_descriptor = wgpu::BindGroupDescriptor {
			layout:  &texture_bind_group_layout,
			entries: &[
				wgpu::BindGroupEntry {
					binding:  0,
					resource: wgpu::BindingResource::TextureView(&diffuse_texture_view),
				},
				wgpu::BindGroupEntry {
					binding:  1,
					resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
				},
			],
			label:   Some("diffuse_bind_group"),
		};
		let bind_group = device.create_bind_group(&bind_group_descriptor);


		Ok(Self {
			args,
			event_loop,
			window,
			renderer,
			geometries,
			render_pipeline,
			indices,
			uniform_buffer,
		})
	}

	/// Runs the app 'till completion
	pub fn run(mut self) -> Result<(), anyhow::Error> {
		// Start the renderer thread
		thread::scope(|s| {
			// Spawn the renderer thread
			s.builder()
				.name("Renderer thread".to_owned())
				.spawn(Self::renderer_thread(
					&self.renderer,
					&mut self.geometries,
					&self.args,
					&self.indices,
					self.window,
					&self.render_pipeline,
				))
				.context("Unable to start renderer thread")?;

			// Run event loop in this thread until we quit
			self.event_loop.run_return(|event, _, control_flow| {
				// Set control for to wait for next event, since we're not doing
				// anything else on the main thread
				*control_flow = EventLoopControlFlow::Wait;

				// Then handle the event
				match event {
					Event::WindowEvent { event, .. } => match event {
						WindowEvent::Resized(size) => self.renderer.resize(size),
						WindowEvent::CloseRequested | WindowEvent::Destroyed => {
							*control_flow = EventLoopControlFlow::Exit;
						},
						_ => (),
					},
					_ => (),
				}
			});

			Ok(())
		})
		.map_err(|err| anyhow::anyhow!("Unable to start all threads and run event loop: {:?}", err))?
	}

	/// Returns the function to run in the renderer thread
	fn renderer_thread<'a>(
		renderer: &'a Renderer, geometries: &'a mut [Geometry], args: &'a Args, indices: &'a wgpu::Buffer,
		window: &'a Window, render_pipeline: &'a wgpu::RenderPipeline,
	) -> impl FnOnce(&thread::Scope) + 'a {
		move |_| loop {
			// Duration we're sleep
			let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

			loop {
				// Render
				let start_time = Instant::now();
				let res = renderer.render(|encoder, view| {
					let render_pass_descriptor = wgpu::RenderPassDescriptor {
						label:                    Some("Render pass"),
						color_attachments:        &[wgpu::RenderPassColorAttachment {
							view,
							resolve_target: None,
							ops: wgpu::Operations {
								load:  wgpu::LoadOp::Clear(wgpu::Color {
									r: 0.1,
									g: 0.2,
									b: 0.3,
									a: 1.0,
								}),
								store: true,
							},
						}],
						depth_stencil_attachment: None,
					};
					let mut render_pass = encoder.begin_render_pass(&render_pass_descriptor);
					render_pass.set_pipeline(render_pipeline);

					for geometry in &mut *geometries {
						Self::draw(
							&mut render_pass,
							geometry,
							args.fade_point,
							indices,
							window.inner_size(),
						);
					}
				});
				let frame_duration = Instant::now().saturating_duration_since(start_time);

				// Check if successful
				match res {
					Ok(()) => log::debug!("Took {frame_duration:?} to render"),
					Err(err) => log::warn!("Unable to render: {err:?}"),
				}

				// Then sleep until next frame
				if let Some(duration) = sleep_duration.checked_sub(frame_duration) {
					std::thread::sleep(duration);
				}
			}
		}
	}

	/// Draws
	#[allow(clippy::cast_precision_loss)] // Image and window sizes are far below 2^23
	fn draw<'a>(
		render_pass: &mut wgpu::RenderPass<'a>, geometry_state: &'a mut Geometry, fade: f32, indices: &'a wgpu::Buffer,
		window_size: PhysicalSize<u32>,
	) {
		// Calculate the base alpha and progress to apply to the images
		let cur_progress = geometry_state.progress;
		let (base_alpha, next_progress) = match cur_progress {
			f if f >= fade => ((cur_progress - fade) / (1.0 - fade), cur_progress - fade),
			_ => (0.0, 0.0),
		};

		let (cur, next) = match &mut geometry_state.image {
			GeometryImage::Empty => (None, None),
			GeometryImage::PrimaryOnly(cur) | GeometryImage::Swapped { cur, .. } => {
				(Some((cur, 1.0, cur_progress)), None)
			},
			GeometryImage::Both { cur, next } => (
				Some((cur, 1.0 - base_alpha, cur_progress)),
				Some((next, base_alpha, next_progress)),
			),
		};

		// Then draw
		let geometry = geometry_state.geometry;
		for (image, alpha, progress) in [cur, next].into_iter().flatten() {
			// Calculate the matrix for the geometry
			let x_scale = geometry.size[0] as f32 / window_size.width as f32;
			let y_scale = geometry.size[1] as f32 / window_size.height as f32;

			let x_offset = geometry.pos[0] as f32 / window_size.width as f32;
			let y_offset = geometry.pos[1] as f32 / window_size.height as f32;

			let matrix = Matrix4::from_translation(Vector3::new(
				-1.0 + x_scale + 2.0 * x_offset,
				1.0 - y_scale - 2.0 * y_offset,
				0.0,
			)) * Matrix4::from_nonuniform_scale(x_scale, -y_scale, 1.0);

			// Setup the uniforms with all the data


			let texture_offset = image.uvs.offset(progress);
			let uniforms = Uniforms {
				matrix: *<_ as AsRef<[[f32; 4]; 4]>>::as_ref(&matrix),
				texture_offset,
				alpha,
			};
			/*
			let sampler = image.texture.sampled();

			let uniforms = glium::uniform! {
				mat: ,
				tex_sampler: sampler,
				tex_offset: tex_offset,
				alpha: alpha,
			};

			// And draw
			let draw_parameters = glium::DrawParameters {
				blend: glium::Blend::alpha_blending(),
				..glium::DrawParameters::default()
			};
			render_pass
				.draw(&image.vertex_buffer, indices, program, &uniforms, &draw_parameters)
				.context("Unable to draw")?;
			*/

			render_pass.set_vertex_buffer(0, image.vertices.slice(..));
			render_pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
			render_pass.draw_indexed(0..6, 0, 0..1);
		}
	}
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


#[derive(Debug, Copy, Clone)]
#[derive(bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
struct Uniforms {
	/// Matrix
	matrix: [[f32; 4]; 4],

	//tex_sampler: sampler,
	/// Texture offset
	texture_offset: [f32; 2],

	/// Image alpha
	alpha: f32,
}

impl Default for Uniforms {
	fn default() -> Self {
		Self {
			matrix:         [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [
				0.0, 0.0, 0.0, 1.0,
			]],
			texture_offset: [0.0; 2],
			alpha:          1.0,
		}
	}
}

impl Uniforms {
	fn new(matrix: [[f32; 4]; 4], texture_offset: [f32; 2], alpha: f32) -> Self {
		Self {
			matrix,
			texture_offset,
			alpha,
		}
	}
}
