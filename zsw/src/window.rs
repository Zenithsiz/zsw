//! Winit initialization

// Imports
#[cfg(target_os = "linux")]
use winit::platform::x11::{WindowAttributesExtX11, WindowType};
use {
	cgmath::{Point2, Vector2},
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		event_loop::ActiveEventLoop,
		window::{Window, WindowAttributes},
	},
	zsw_util::Rect,
	zutil_app_error::{AppError, Context},
};

/// Creates the window, as well as the associated event loop
pub fn create(event_loop: &ActiveEventLoop) -> Result<Window, AppError> {
	// Find the window geometry
	// Note: We just merge all monitors' geometry.
	let window_geometry = event_loop
		.available_monitors()
		.map(|monitor| self::monitor_geometry(&monitor))
		.reduce(Rect::merge)
		.context("No monitors found")?;
	tracing::debug!(?window_geometry, "Found window geometry");

	// Start building the window
	let window_attrs = WindowAttributes::default()
		.with_title("zsw")
		.with_position(PhysicalPosition {
			x: window_geometry.pos[0],
			y: window_geometry.pos[1],
		})
		.with_inner_size(PhysicalSize {
			width:  window_geometry.size[0],
			height: window_geometry.size[1],
		})
		.with_decorations(false);

	// If on linux x11, add the `Desktop`
	// TODO: Wayland, windows and macos?
	#[cfg(target_os = "linux")]
	let window_attrs = window_attrs.with_x11_window_type(vec![WindowType::Desktop]);

	// Finally build the window
	let window = event_loop
		.create_window(window_attrs)
		.context("Unable to build window")?;

	Ok(window)
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
