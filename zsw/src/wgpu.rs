//! Wgpu
//!
//! This module serves as a high-level interface for `wgpu`.
//!
//! The main entry point is the [`Wgpu`] type, which is used
//! to interface with `wgpu`.
//!
//! This allows the application to not be exposed to the verbose
//! details of `wgpu` and simply use the defaults [`Wgpu`] offers,
//! which are tailored for this application.

// Imports
use {
	crate::util::{extse::ParkingLotMutexSe, MightBlock},
	anyhow::Context,
	crossbeam::atomic::AtomicCell,
	parking_lot::Mutex,
	pollster::FutureExt,
	std::marker::PhantomData,
	wgpu::TextureFormat,
	winit::{dpi::PhysicalSize, window::Window},
	zsw_side_effect_macros::side_effect,
};

/// Surface
// Note: Exists so we may lock both the surface and size behind
//       the same mutex, to ensure resizes are atomic in regards
//       to code using the surface.
//       See the note on the surface size on why this is important.
#[derive(Debug)]
pub struct Surface {
	/// Surface
	surface: wgpu::Surface,

	/// Surface size
	// Note: We keep the size ourselves instead of using the inner
	//       window size because the window resizes asynchronously
	//       from us, so it's possible for the window sizes to be
	//       wrong relative to the surface size.
	//       Wgpu validation code can panic if the size we give it
	//       is invalid (for example, during scissoring), so we *must*
	//       ensure this size is the surface's actual size.
	size: PhysicalSize<u32>,
}

/// Wgpu interface
// TODO: Figure out if drop order matters here. Dropping the surface after the device/queue
//       seems to not result in any panics, but it might be worth checking, especially if we
//       ever need to "restart" `wgpu` in any scenario without restarting the application.
#[derive(Debug)]
pub struct Wgpu<'window> {
	/// Device
	// TODO: There exists a `Device::poll` method, but I'm not sure if we should
	//       have to call that? Seems to be used for async, but we don't use any
	//       of the async methods and unfortunately the polling seems to be busy
	//       waiting even in `Wait` mode, so creating a new thread to poll whenever
	//       doesn't work well without a sleep, which would defeat the point of
	//       polling it in another thread instead of on the main thread whenever
	//       an event is received.
	device: wgpu::Device,

	/// Queue
	queue: wgpu::Queue,

	/// Surface
	// Note: Behind a mutex because, once we get the surface texture, calling most other
	//       methods, such as `configure` will cause `wgpu` to panic due to the texture
	//       being active.
	//       For this reason we lock the surface while rendering to ensure no changes happen
	//       and no panics can occur.
	surface: Mutex<Surface>,

	/// Surface texture format
	///
	/// Used on each resize, so we configure the surface with the same texture format each time.
	surface_texture_format: TextureFormat,

	/// Queued resize
	///
	/// Will be `None` if no resizes are queued.
	// Note: We queue resizes for 2 reasons:
	//       1. So that multiple window resizes per frame only trigger an actual surface resize to
	//          improve performance.
	//       2. So that resizing may be done before rendering, so we can have a synchronizes-with
	//          relation between surface resizes and drawing. This ensures we never resize the surface
	//          without showing the user at least 1 frame of the resized surface.
	queued_resize: AtomicCell<Option<PhysicalSize<u32>>>,

	/// Window lifetime
	// Note: Our surface must outlive the window, so we make sure of it using the `'window` lifetime
	window_phantom: PhantomData<&'window Window>,
}

impl<'window> Wgpu<'window> {
	/// Creates the `wgpu` wrapper given the window to create it in.
	pub fn new(window: &'window Window) -> Result<Self, anyhow::Error> {
		// Create the surface and adapter
		// SAFETY: Due to our lifetime, we ensure the window outlives us and thus the surface
		let (surface, adapter) = unsafe { self::create_surface_and_adapter(window)? };

		// Then create the device and it's queue
		let (device, queue) = self::create_device(&adapter).block_on()?;

		// Configure the surface and get the preferred texture format and surface size
		let (surface_texture_format, surface_size) =
			self::configure_window_surface(window, &surface, &adapter, &device)?;

		log::info!("Successfully initialized");
		Ok(Self {
			surface: Mutex::new(Surface {
				surface,
				size: surface_size,
			}),
			device,
			queue,
			surface_texture_format,
			queued_resize: AtomicCell::new(None),
			window_phantom: PhantomData,
		})
	}

	/// Returns the wgpu device
	pub const fn device(&self) -> &wgpu::Device {
		&self.device
	}

	/// Returns the wgpu queue
	pub const fn queue(&self) -> &wgpu::Queue {
		&self.queue
	}

	/// Returns the current surface's size
	///
	/// # Blocking
	/// Blocks until any calls to [`Wgpu::render`] are finished.
	///
	/// # Warning
	/// This surface size might change at any time, so you shouldn't
	/// use it on `wgpu` operations that might panic on wrong surface
	/// sizes.
	#[side_effect(MightBlock)]
	pub fn surface_size(&self) -> PhysicalSize<u32> {
		// DEADLOCK: Caller is responsible for avoiding deadlocks
		self.surface.lock_se().allow::<MightBlock>().size
	}

	/// Returns the surface texture format
	pub const fn surface_texture_format(&self) -> wgpu::TextureFormat {
		self.surface_texture_format
	}

	/// Resizes the underlying surface
	///
	/// The resize isn't executed immediately. Instead, it is
	/// queued to happen at the start of the next render.
	///
	/// This means you can call this method whenever you receive
	/// the resize event from the window.
	pub fn resize(&self, size: PhysicalSize<u32>) {
		// Queue the resize
		self.queued_resize.store(Some(size));
	}

	/// Renders a frame using `f`
	///
	/// # Blocking
	/// Blocks until any calls to [`Wgpu::surface_size`] are finished.
	///
	/// # Callback
	/// Callback `f` receives the command encoder and the surface texture / size. This allows you to
	/// start render passes to the surface texture.
	///
	/// Once `f` returns, all commands are sent via the `wgpu` queue and the surface is presented.
	///
	/// If any resize is queued, it will be executed *before* the frame starts, so the frame will start
	/// with the new size.
	#[side_effect(MightBlock)]
	pub fn render(
		&self,
		f: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView, PhysicalSize<u32>) -> Result<(), anyhow::Error>,
	) -> Result<(), anyhow::Error> {
		// Note: We want to keep the surface locked until the end of the
		//       method to prevent any possible changes from another thread
		//       mid-frame, which could cause panics in `wgpu` validation.
		// DEADLOCK: Caller is responsible for avoiding deadlocks
		let mut surface = self.surface.lock_se().allow::<MightBlock>();

		// Check for resizes
		if let Some(size) = self.queued_resize.take() {
			log::info!("Resizing to {size:?}");
			if size.width > 0 && size.height > 0 {
				// Update our surface
				let config = self::window_surface_configuration(self.surface_texture_format, size);
				surface.surface.configure(&self.device, &config);
				surface.size = size;
			}
		}

		// And then get the surface texture
		let surface_texture = surface
			.surface
			.get_current_texture()
			.context("Unable to retrieve current texture")?;
		let surface_view_descriptor = wgpu::TextureViewDescriptor {
			label: Some("[zsw] Window surface texture view"),
			..wgpu::TextureViewDescriptor::default()
		};
		let surface_texture_view = surface_texture.texture.create_view(&surface_view_descriptor);

		// Then create an encoder for our frame
		let encoder_descriptor = wgpu::CommandEncoderDescriptor {
			label: Some("[zsw] Frame render command encoder"),
		};
		let mut encoder = self.device.create_command_encoder(&encoder_descriptor);

		// And render using `f`
		f(&mut encoder, &surface_texture_view, surface.size).context("Unable to render")?;

		// Finally submit everything to the queue and present the surface's texture
		self.queue.submit([encoder.finish()]);
		surface_texture.present();

		Ok(())
	}
}

/// Configures the window surface and returns the preferred surface texture format
fn configure_window_surface(
	window: &Window,
	surface: &wgpu::Surface,
	adapter: &wgpu::Adapter,
	device: &wgpu::Device,
) -> Result<(TextureFormat, PhysicalSize<u32>), anyhow::Error> {
	// Get the format
	let surface_texture_format = surface
		.get_preferred_format(adapter)
		.context("Unable to query preferred format")?;
	log::debug!("Found preferred surface format: {surface_texture_format:?}");

	// Then configure it
	let surface_size = window.inner_size();
	let config = self::window_surface_configuration(surface_texture_format, surface_size);
	log::debug!("Configuring surface with {config:?}");
	surface.configure(device, &config);

	Ok((surface_texture_format, surface_size))
}

/// Creates the device
async fn create_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), anyhow::Error> {
	// Request the device without any features
	let device_descriptor = wgpu::DeviceDescriptor {
		label:    Some("[zsw] Device"),
		features: wgpu::Features::empty(),
		limits:   wgpu::Limits::default(),
	};
	log::debug!("Requesting wgpu device (descriptor: {device_descriptor:#?})");
	let (device, queue) = adapter
		.request_device(&device_descriptor, None)
		.await
		.context("Unable to request device")?;

	Ok((device, queue))
}

/// Creates the surface and adapter
///
/// # Safety
/// The returned surface *must* be dropped before the window.
unsafe fn create_surface_and_adapter(window: &Window) -> Result<(wgpu::Surface, wgpu::Adapter), anyhow::Error> {
	// Get an instance with any backend
	let backends = wgpu::Backends::all();
	log::debug!("Requesting wgpu instance (backends: {backends:?})");
	let instance = wgpu::Instance::new(backends);

	// Create the surface
	// SAFETY: Caller promises the window outlives the surface
	log::debug!("Creating wgpu surface (window: {window:?})");
	let surface = unsafe { instance.create_surface(window) };

	// Then request the adapter
	let adapter_options = wgpu::RequestAdapterOptions {
		power_preference:       wgpu::PowerPreference::default(),
		force_fallback_adapter: false,
		compatible_surface:     Some(&surface),
	};
	log::debug!("Requesting wgpu adapter (options: {adapter_options:#?})");
	let adapter = instance
		.request_adapter(&adapter_options)
		.block_on()
		.context("Unable to request adapter")?;

	Ok((surface, adapter))
}

/// Returns the window surface configuration
const fn window_surface_configuration(
	surface_texture_format: TextureFormat,
	size: PhysicalSize<u32>,
) -> wgpu::SurfaceConfiguration {
	wgpu::SurfaceConfiguration {
		usage:        wgpu::TextureUsages::RENDER_ATTACHMENT,
		format:       surface_texture_format,
		width:        size.width,
		height:       size.height,
		present_mode: wgpu::PresentMode::Mailbox,
	}
}
