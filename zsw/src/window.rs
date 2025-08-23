//! Winit initialization

// Imports
use {
	cgmath::{Point2, Vector2},
	winit::{
		event_loop::ActiveEventLoop,
		window::{Window, WindowAttributes},
	},
	zsw_util::Rect,
	zutil_app_error::{AppError, Context},
};

/// Application window
#[derive(Debug)]
pub struct AppWindow {
	/// Monitor name
	pub monitor_name: String,

	/// Monitor geometry
	pub monitor_geometry: Rect<i32, u32>,

	/// Window
	pub window: Window,
}

/// Creates the windows for each monitor, as well as the associated event loop
pub fn create(event_loop: &ActiveEventLoop) -> Result<Vec<AppWindow>, AppError> {
	event_loop
		.available_monitors()
		.enumerate()
		.map(|(monitor_idx, monitor)| {
			let monitor_name = monitor
				.name()
				.unwrap_or_else(|| format!("Monitor #{}", monitor_idx + 1));

			let monitor_geometry = self::monitor_geometry(&monitor);
			tracing::debug!("Found monitor {monitor_name:?} geometry: {monitor_geometry}");

			// Start building the window
			// TODO: `AlwaysOnBottom` doesn't work on wayland
			let window_attrs = WindowAttributes::default()
				.with_title("zsw")
				.with_position(monitor.position())
				.with_inner_size(monitor.size())
				.with_resizable(false)
				.with_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(monitor))))
				.with_window_level(winit::window::WindowLevel::AlwaysOnBottom)
				.with_transparent(true)
				.with_decorations(false);

			// Finally build the window
			let window = event_loop
				.create_window(window_attrs)
				.context("Unable to build window")?;

			Ok(AppWindow {
				monitor_name,
				monitor_geometry,
				window,
			})
		})
		.collect::<Result<_, AppError>>()
		.context("Unable to create all windows")
}

/// Returns a monitor's geometry
fn monitor_geometry(monitor: &winit::monitor::MonitorHandle) -> Rect<i32, u32> {
	let monitor_pos = monitor.position();
	let monitor_size = monitor.size();
	Rect {
		pos:  Point2::new(monitor_pos.x, monitor_pos.y),
		size: Vector2::new(monitor_size.width, monitor_size.height),
	}
}
