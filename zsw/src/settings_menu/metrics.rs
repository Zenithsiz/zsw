//! Metrics tab

// Imports
use {
	crate::metrics::{FrameTime, Metrics},
	core::time::Duration,
	winit::window::WindowId,
};

/// Draws the metrics tab
pub fn draw_metrics_tab(ui: &mut egui::Ui, metrics: &Metrics) {
	let frame_times = metrics.frame_times();

	// Use the first window as the current window if we hadn't selected one yet
	let cur_window_id = super::get_data::<Option<WindowId>>(ui, "metrics-tab-cur-window");
	let mut cur_window_id = cur_window_id.lock();
	if cur_window_id.is_none() &&
		let Some((&window_id, _)) = frame_times.first_key_value()
	{
		*cur_window_id = Some(window_id);
	}


	let stack_charts = super::get_data_with_default::<bool>(ui, "metrics-tab-chart-stacks", || true);
	let mut stack_charts = stack_charts.lock();
	egui::ScrollArea::horizontal().show(ui, |ui| {
		egui::ComboBox::from_id_salt("Window")
			.selected_text(format!("{cur_window_id:?}"))
			.show_ui(ui, |ui| {
				// TODO: Get windows through another way?
				for &window_id in frame_times.keys() {
					ui.selectable_value(&mut *cur_window_id, Some(window_id), format!("{window_id:?}"));
				}
			});

		ui.horizontal(|ui| {
			let Some(window_id) = *cur_window_id else { return };
			let mut is_paused = metrics.frame_times_is_paused(window_id);
			if ui.toggle_value(&mut is_paused, "Pause").changed() {
				metrics.frame_times_pause(window_id, is_paused);
			}

			ui.toggle_value(&mut stack_charts, "Stack charts");
		});
	});

	let Some(window_id) = *cur_window_id else { return };
	let Some(frame_times) = frame_times.get(&window_id) else {
		return;
	};

	let mut charts = vec![];
	for duration_idx in 0..6 {
		let bars = frame_times
			.iter()
			.enumerate()
			.map(|(frame_idx, frame_time)| {
				egui_plot::Bar::new(
					frame_idx as f64,
					self::frame_time_non_cumulative(frame_time, duration_idx).as_millis_f64(),
				)
			})
			.collect();

		let mut chart = egui_plot::BarChart::new(self::frame_time_name(duration_idx), bars);
		if *stack_charts {
			chart = chart.stack_on(&charts.iter().collect::<Vec<_>>());
		}
		charts.push(chart);
	}

	egui_plot::Plot::new("Frame times")
		.legend(egui_plot::Legend::default())
		.clamp_grid(true)
		.y_axis_label("Time")
		.show(ui, |plot_ui| {
			for chart in charts {
				plot_ui.bar_chart(chart);
			}
		});
}

/// Returns the duration with index `idx`
pub fn frame_time(frame_time: &FrameTime, idx: usize) -> Duration {
	match idx {
		0 => frame_time.paint_egui,
		1 => frame_time.render_start,
		2 => frame_time.render_panels,
		3 => frame_time.render_egui,
		4 => frame_time.render_finish,
		5 => frame_time.resize,
		_ => Duration::ZERO,
	}
}

/// Returns the non-cumulative duration with index `idx`
pub fn frame_time_non_cumulative(frame_time: &FrameTime, idx: usize) -> Duration {
	match idx {
		0 => self::frame_time(frame_time, 0),
		_ => self::frame_time(frame_time, idx) - self::frame_time(frame_time, idx - 1),
	}
}

/// Returns the name for a frame time duration
pub fn frame_time_name(idx: usize) -> &'static str {
	match idx {
		0 => "Paint egui",
		1 => "Render start",
		2 => "Render panels",
		3 => "Render egui",
		4 => "Render finish",
		5 => "Resize",
		_ => "Unknown",
	}
}
