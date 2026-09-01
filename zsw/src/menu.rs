//! Menu

#![allow(unused_results)] // Egui produces a lot of results we don't need to use

mod panels;
mod profiles;

use {
	crate::{AppEvent, panel::Panels, playlist::Playlists, profile::Profiles},
	core::{ops::RangeInclusive, time::Duration},
	egui::Widget,
	std::path::Path,
	strum::IntoEnumIterator,
	winit::event_loop::EventLoopProxy,
	zsw_util::{AppError, Rect},
	zsw_wgpu::WgpuRenderer,
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
	#[expect(clippy::too_many_arguments, reason = "TODO: Merge some arguments")]
	pub fn draw(
		&mut self,
		ctx: &egui::Context,
		wgpu_renderer: &WgpuRenderer,
		playlists: &Playlists,
		profiles: &Profiles,
		panels: &mut Panels,
		event_loop_proxy: &EventLoopProxy<AppEvent>,
		window_geometry: Rect<i32, u32>,
	) {
		let mut egui_window = egui::Window::new("Menu");

		// Open the window at the mouse if pressed
		if !ctx.is_pointer_over_egui() &&
			ctx.input(|input| input.pointer.secondary_pressed()) &&
			let Some(pointer_pos) = ctx.input(|input| input.pointer.latest_pos())
		{
			egui_window = egui_window.fixed_pos(pointer_pos);
			self.open = true;
		}

		egui_window.open(&mut self.open).show(ctx, |ui| {
			ui.horizontal(|ui| {
				for tab in Tab::iter() {
					ui.selectable_value(&mut self.cur_tab, tab, tab.to_string());
				}
			});
			ui.separator();

			match self.cur_tab {
				Tab::Panels => panels::draw_panels_tab(ui, wgpu_renderer, panels, window_geometry),
				Tab::Profiles => profiles::draw_profiles_tab(ui, wgpu_renderer, playlists, profiles, panels),
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
		// TODO: Not use lossy conversion to display it?
		if ui.link(path.to_string_lossy()).clicked() &&
			let Err(err) = opener::open(path)
		{
			let err = AppError::new(&err);
			tracing::warn!("Unable to open file {path:?}: {err:?}");
		}
	});
}

/// Draws a geometry rectangle
fn draw_rect(ui: &mut egui::Ui, geometry: Rect<i32, u32>) {
	ui.label(format!(
		"{}x{}+{}+{}",
		geometry.size.x, geometry.size.y, geometry.pos.x, geometry.pos.y
	));
}

/// Draws a duration slider
// TODO: Allow setting the clamping mode by using a builder instead
// TODO: This always modifies the value each frame and rounds it.
fn draw_duration(ui: &mut egui::Ui, duration: &mut Duration, range: RangeInclusive<Duration>) {
	let mut secs = duration.as_secs_f32();

	let start = range.start().as_secs_f32();
	let end = range.end().as_secs_f32();
	egui::Slider::new(&mut secs, start..=end)
		.custom_formatter(|secs, _| {
			// Note: We round any durations to the nearest millisecond to avoid displaying
			//       numbers that are too big
			let duration = Duration::from_secs_f64(secs);
			let nanos_per_ms = Duration::from_millis(1).subsec_nanos();
			let duration = Duration::new(
				duration.as_secs(),
				duration.subsec_nanos().next_multiple_of(nanos_per_ms),
			);
			humantime::format_duration(duration).to_string()
		})
		.custom_parser(|s| humantime::parse_duration(s).ok().map(|d| d.as_secs_f64()))
		.clamping(egui::SliderClamping::Never)
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

	#[display("Profiles")]
	Profiles,

	#[display("Settings")]
	Settings,
}
