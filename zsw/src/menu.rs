//! Menu

// Lints
#![allow(unused_results)] // Egui produces a lot of results we don't need to use

// Modules
mod displays;
mod metrics;
mod panels;
mod playlists;
mod profiles;

// Imports
use {
	crate::{
		AppEvent,
		display::Displays,
		metrics::Metrics,
		panel::Panels,
		playlist::Playlists,
		profile::Profiles,
		window::WindowMonitorNames,
	},
	core::{ops::RangeInclusive, str::FromStr, time::Duration},
	egui::Widget,
	std::{
		path::Path,
		sync::{Arc, nonpoison::Mutex},
	},
	strum::IntoEnumIterator,
	winit::{dpi::LogicalPosition, event_loop::EventLoopProxy},
	zsw_util::{AppError, DurationDisplay, Rect},
	zsw_wgpu::Wgpu,
};

/// Menu
#[derive(Debug)]
pub struct Menu {
	/// If open
	open: bool,

	/// Current tab
	cur_tab: Tab,
}

impl Menu {
	/// Creates the menu
	pub fn new() -> Self {
		Self {
			open:    false,
			cur_tab: Tab::Panels,
		}
	}

	/// Draws the menu
	pub fn draw(
		&mut self,
		ctx: &egui::Context,
		wgpu: &Wgpu,
		displays: &Arc<Displays>,
		playlists: &Arc<Playlists>,
		profiles: &Arc<Profiles>,
		panels: &Arc<Panels>,
		metrics: &Metrics,
		window_monitor_names: &WindowMonitorNames,
		event_loop_proxy: &EventLoopProxy<AppEvent>,
		cursor_pos: Option<LogicalPosition<f32>>,
		window_geometry: Rect<i32, u32>,
	) {
		// Create the window
		let mut egui_window = egui::Window::new("Menu");

		// Open it at the mouse if pressed
		if let Some(cursor_pos) = cursor_pos &&
			!ctx.is_pointer_over_area() &&
			ctx.input(|input| input.pointer.button_clicked(egui::PointerButton::Secondary))
		{
			egui_window = egui_window.current_pos(egui::pos2(cursor_pos.x, cursor_pos.y));
			self.open = true;
		}

		// Then render it
		egui_window.open(&mut self.open).show(ctx, |ui| {
			ui.horizontal(|ui| {
				for tab in Tab::iter() {
					ui.selectable_value(&mut self.cur_tab, tab, tab.to_string());
				}
			});
			ui.separator();

			match self.cur_tab {
				Tab::Panels => panels::draw_panels_tab(ui, wgpu, panels, window_geometry),
				Tab::Displays => displays::draw_displays_tab(ui, displays),
				Tab::Playlists => playlists::draw_playlists_tab(ui, playlists),
				Tab::Profiles => profiles::draw_profiles_tab(ui, displays, playlists, profiles, panels),
				Tab::Metrics => metrics::draw_metrics_tab(ui, metrics, window_monitor_names),
				Tab::Settings => self::draw_settings_tab(ui, event_loop_proxy),
			}
		});
	}
}


/// Draws the settings tab
fn draw_settings_tab(ui: &mut egui::Ui, event_loop_proxy: &EventLoopProxy<AppEvent>) {
	if ui.button("Quit").clicked() {
		event_loop_proxy
			.send_event(crate::AppEvent::Shutdown)
			.expect("Unable to send shutdown event to event loop");
	}
}

/// Draws an openable path
fn draw_openable_path(ui: &mut egui::Ui, path: &Path) {
	ui.horizontal(|ui| {
		ui.label("Path: ");
		// TODO: Not use lossy conversion to display it?
		if ui.link(path.to_string_lossy()).clicked() &&
			let Err(err) = opener::open(path)
		{
			let err = AppError::new(&err);
			tracing::warn!("Unable to open file {path:?}: {}", err.pretty());
		}
	});
}


/// Draws a geometry rectangle
fn draw_rect(ui: &mut egui::Ui, geometry: &mut Rect<i32, u32>) {
	ui.horizontal(|ui| {
		egui::DragValue::new(&mut geometry.size.x).speed(10).ui(ui);
		ui.label("x");
		egui::DragValue::new(&mut geometry.size.y).speed(10).ui(ui);
		ui.label("+");
		egui::DragValue::new(&mut geometry.pos.x).speed(10).ui(ui);
		ui.label("+");
		egui::DragValue::new(&mut geometry.pos.y).speed(10).ui(ui);
	});
}

/// Draws a duration slider
// TODO: Allow setting the clamping mode by using a builder instead
fn draw_duration(ui: &mut egui::Ui, duration: &mut Duration, range: RangeInclusive<Duration>) {
	let mut secs = duration.as_secs_f32();

	let start = range.start().as_secs_f32();
	let end = range.end().as_secs_f32();
	egui::Slider::new(&mut secs, start..=end)
		.custom_formatter(|secs, _| DurationDisplay(Duration::from_secs_f64(secs)).to_string())
		.custom_parser(|s| DurationDisplay::from_str(s).ok().map(|d| d.0.as_secs_f64()))
		.clamping(egui::SliderClamping::Edits)
		.ui(ui);
	*duration = Duration::from_secs_f32(secs);
}

/// Tab
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[derive(derive_more::Display)]
#[derive(strum::EnumIter)]
enum Tab {
	#[display("Panels")]
	Panels,

	#[display("Displays")]
	Displays,

	#[display("Playlists")]
	Playlists,

	#[display("Profiles")]
	Profiles,

	#[display("Metrics")]
	Metrics,

	#[display("Settings")]
	Settings,
}

/// Gets an `Arc<Mutex<T>>` from the egui data with id `id`
fn get_data<T>(ui: &egui::Ui, id: impl Into<egui::Id>) -> Arc<Mutex<T>>
where
	T: Default + Send + 'static,
{
	self::get_data_with_default(ui, id, T::default)
}

/// Gets an `Arc<Mutex<T>>` from the egui data with id `id` with a default value
fn get_data_with_default<T>(ui: &egui::Ui, id: impl Into<egui::Id>, default: impl FnOnce() -> T) -> Arc<Mutex<T>>
where
	T: Send + 'static,
{
	ui.data_mut(|map| {
		let value = map.get_persisted_mut_or_insert_with(id.into(), || Arc::new(Mutex::new(default())));
		Arc::clone(value)
	})
}
