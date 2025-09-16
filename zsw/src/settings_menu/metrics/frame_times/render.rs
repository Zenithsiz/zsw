//! Render frame time metrics

// Imports
use crate::metrics::{FrameTimes, RenderFrameTime};

/// Draws the render frame times
pub fn draw(ui: &mut egui::Ui, render_frame_times: &mut FrameTimes<RenderFrameTime>) {
	let settings = super::draw_frame_time_settings(ui, render_frame_times);

	let mut charts = vec![];
	for duration_idx in 0..6 {
		let chart = super::add_frame_time_chart(
			render_frame_times,
			settings.is_histogram,
			settings.histogram_time_scale,
			settings.stack_charts,
			&charts,
			match duration_idx {
				0 => "Paint egui",
				1 => "Render start",
				2 => "Render panels",
				3 => "Render egui",
				4 => "Render finish",
				5 => "Resize",
				_ => "Unknown",
			},
			|frame_time| match duration_idx {
				0 => frame_time.paint_egui,
				1 => frame_time.render_start,
				2 => frame_time.render_panels,
				3 => frame_time.render_egui,
				4 => frame_time.render_finish,
				5 => frame_time.resize,
				_ => unreachable!(),
			},
		);
		charts.push(chart);
	}

	let plot = egui_plot::Plot::new("Render frame times")
		.legend(egui_plot::Legend::default())
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
