//! Winit initialization

// Imports
use {
	app_error::Context,
	cgmath::{Point2, Vector2},
	std::{collections::HashMap, sync::nonpoison::Mutex},
	winit::{
		event_loop::ActiveEventLoop,
		monitor::MonitorHandle,
		window::{Fullscreen, Window, WindowAttributes, WindowId, WindowLevel},
	},
	zsw_util::{AppError, Rect},
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
pub fn create(event_loop: &ActiveEventLoop, transparent_windows: bool) -> Result<Vec<AppWindow>, AppError> {
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
				.with_fullscreen(Some(Fullscreen::Borderless(Some(monitor))))
				.with_window_level(WindowLevel::AlwaysOnBottom)
				.with_transparent(transparent_windows)
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
fn monitor_geometry(monitor: &MonitorHandle) -> Rect<i32, u32> {
	let monitor_pos = monitor.position();
	let monitor_size = monitor.size();
	Rect {
		pos:  Point2::new(monitor_pos.x, monitor_pos.y),
		size: Vector2::new(monitor_size.width, monitor_size.height),
	}
}

/// Window monitor names
#[derive(Debug)]
pub struct WindowMonitorNames {
	/// Inner
	inner: Mutex<HashMap<WindowId, String>>,
}

impl WindowMonitorNames {
	/// Creates an empty map of window names
	pub fn new() -> Self {
		Self {
			inner: Mutex::new(HashMap::new()),
		}
	}

	/// Adds a window's monitor name
	pub fn add(&self, window_id: WindowId, name: impl Into<String>) {
		_ = self.inner.lock().insert(window_id, name.into())
	}

	/// Gets a window's monitor name.
	pub fn get(&self, window_id: WindowId) -> Option<String> {
		self.inner.lock().get(&window_id).cloned()
	}
}
