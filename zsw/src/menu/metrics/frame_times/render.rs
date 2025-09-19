//! Render frame time metrics

// Imports
use {
	crate::metrics::{FrameTimes, RenderFrameTime},
	std::time::Duration,
	strum::IntoEnumIterator,
};

/// Draws the render frame times
pub fn draw(ui: &mut egui::Ui, render_frame_times: &mut FrameTimes<RenderFrameTime>) {
	let display = super::draw_display_settings(ui, render_frame_times);

	super::draw_plot(ui, render_frame_times, &display, DurationIdx::iter());
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[derive(strum::EnumIter)]
enum DurationIdx {
	WaitNextFrame,
	PaintEgui,
	RenderStart,
	RenderPanels,
	RenderEgui,
	RenderFinish,
	HandleEvents,
}

impl super::DurationIdx<RenderFrameTime> for DurationIdx {
	fn name(&self) -> String {
		match self {
			Self::WaitNextFrame => "Wait next frame".to_owned(),
			Self::PaintEgui => "Paint egui".to_owned(),
			Self::RenderStart => "Render start".to_owned(),
			Self::RenderPanels => "Render panels".to_owned(),
			Self::RenderEgui => "Render egui".to_owned(),
			Self::RenderFinish => "Render finish".to_owned(),
			Self::HandleEvents => "Handle events".to_owned(),
		}
	}

	fn duration_of(&self, frame_time: &RenderFrameTime) -> Option<Duration> {
		let duration = match self {
			Self::WaitNextFrame => frame_time.wait_next_frame,
			Self::PaintEgui => frame_time.paint_egui,
			Self::RenderStart => frame_time.render_start,
			Self::RenderPanels => frame_time.render_panels,
			Self::RenderEgui => frame_time.render_egui,
			Self::RenderFinish => frame_time.render_finish,
			Self::HandleEvents => frame_time.handle_events,
		};

		Some(duration)
	}
}
