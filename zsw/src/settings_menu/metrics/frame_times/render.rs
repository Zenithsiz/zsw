//! Render frame time metrics

// Imports
use {
	crate::metrics::{FrameTimes, RenderFrameTime},
	std::time::Duration,
	strum::IntoEnumIterator,
};

/// Draws the render frame times
pub fn draw(ui: &mut egui::Ui, render_frame_times: &mut FrameTimes<RenderFrameTime>) {
	let settings = super::draw_frame_time_settings(ui, render_frame_times);

	let mut charts = vec![];
	for duration_idx in DurationIdx::iter() {
		let chart = super::add_frame_time_chart(
			render_frame_times,
			settings.is_histogram,
			settings.histogram_time_scale,
			settings.stack_charts,
			&charts,
			&duration_idx,
		);
		charts.push(chart);
	}

	let legend = egui_plot::Legend::default().follow_insertion_order(true);

	let plot = egui_plot::Plot::new("Render frame times")
		.legend(legend)
		.clamp_grid(true);

	let plot = match settings.is_histogram {
		true => plot.x_axis_label("Time (ms)").y_axis_label("Occurrences (normalized)"),
		false => plot.x_axis_label("Frame").y_axis_label("Time (ms)"),
	};

	plot.show(ui, |plot_ui| {
		for chart in charts {
			plot_ui.bar_chart(chart);
		}
	});
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[derive(strum::EnumIter)]
enum DurationIdx {
	PaintEgui,
	RenderStart,
	RenderPanels,
	RenderEgui,
	RenderFinish,
	Resize,
}

impl super::DurationIdx<RenderFrameTime> for DurationIdx {
	fn name(&self) -> String {
		match self {
			Self::PaintEgui => "Paint egui".to_owned(),
			Self::RenderStart => "Render start".to_owned(),
			Self::RenderPanels => "Render panels".to_owned(),
			Self::RenderEgui => "Render egui".to_owned(),
			Self::RenderFinish => "Render finish".to_owned(),
			Self::Resize => "Resize".to_owned(),
		}
	}

	fn duration_of(&self, frame_time: &RenderFrameTime) -> Option<Duration> {
		let duration = match self {
			Self::PaintEgui => frame_time.paint_egui,
			Self::RenderStart => frame_time.render_start,
			Self::RenderPanels => frame_time.render_panels,
			Self::RenderEgui => frame_time.render_egui,
			Self::RenderFinish => frame_time.render_finish,
			Self::Resize => frame_time.resize,
		};

		Some(duration)
	}
}
