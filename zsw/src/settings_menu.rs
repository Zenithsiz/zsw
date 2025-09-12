//! Settings menu

// Lints
#![allow(unused_results)] // Egui produces a lot of results we don't need to use

// Modules
mod displays;
mod panels;
mod profiles;

// Imports
use {
	crate::{AppEvent, display::Displays, panel::Panel, profile::Profiles},
	core::{ops::RangeInclusive, str::FromStr, time::Duration},
	egui::{Widget, mutex::Mutex},
	std::{path::Path, sync::Arc},
	strum::IntoEnumIterator,
	winit::{dpi::LogicalPosition, event_loop::EventLoopProxy},
	zsw_util::{AppError, DurationDisplay, Rect},
	zsw_wgpu::Wgpu,
};

/// Settings menu
#[derive(Debug)]
pub struct SettingsMenu {
	/// If open
	open: bool,

	/// Current tab
	cur_tab: Tab,
}

impl SettingsMenu {
	/// Creates the settings menu
	pub fn new() -> Self {
		Self {
			open:    false,
			cur_tab: Tab::Panels,
		}
	}

	/// Draws the settings menu
	pub fn draw(
		&mut self,
		ctx: &egui::Context,
		wgpu: &Wgpu,
		displays: &Arc<Displays>,
		profiles: &Arc<Profiles>,
		panels: &mut [Panel],
		event_loop_proxy: &EventLoopProxy<AppEvent>,
		cursor_pos: LogicalPosition<f32>,
		window_geometry: Rect<i32, u32>,
	) {
		// Create the window
		let mut egui_window = egui::Window::new("Settings");

		// Open it at the mouse if pressed
		if !ctx.is_pointer_over_area() &&
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
				Tab::Profiles => profiles::draw_profiles_tab(ui, profiles),
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

	#[display("Profiles")]
	Profiles,

	#[display("Settings")]
	Settings,
}

/// Gets an `Arc<Mutex<T>>` from the egui data with id `id`
fn get_data<T>(ui: &egui::Ui, id: impl Into<egui::Id>) -> Arc<Mutex<T>>
where
	T: Default + Send + 'static,
{
	ui.data_mut(|map| {
		let value = map.get_persisted_mut_or_default(id.into());
		Arc::clone(value)
	})
}
