//! Metrics tab

// Imports
use {
	crate::metrics::{Metrics, RenderFrameTime},
	core::time::Duration,
	egui::Widget,
	std::collections::{BTreeSet, HashMap},
	winit::window::WindowId,
};

/// Draws the metrics tab
pub fn draw_metrics_tab(ui: &mut egui::Ui, metrics: &Metrics) {
	// Get the window, otherwise we have nothing to render
	let Some(window_id) = self::render_window_select(ui, metrics) else {
		ui.weak("No window selected");
		return;
	};

	let render_frame_times = metrics.render_frame_times();


	// TODO: Turn this into some enum between histogram / time
	let is_histogram = super::get_data::<bool>(ui, "metrics-tab-histogram");
	let mut is_histogram = is_histogram.lock();

	let histogram_time_scale = super::get_data_with_default::<f64>(ui, "metrics-tab-histogram-time-scale", || 20.0);
	let mut histogram_time_scale = histogram_time_scale.lock();

	let stack_charts = super::get_data_with_default::<bool>(ui, "metrics-tab-chart-stacks", || true);
	let mut stack_charts = stack_charts.lock();

	ui.horizontal(|ui| {
		let mut is_paused = metrics.render_frame_times_is_paused(window_id);
		if ui.toggle_value(&mut is_paused, "Pause").changed() {
			metrics.render_frame_times_pause(window_id, is_paused);
		}

		let mut max_len = metrics.render_frame_times_max_len(window_id);
		ui.horizontal(|ui| {
			ui.label("Maximum frames: ");
			if egui::Slider::new(&mut max_len, 0..=60 * 100).ui(ui).changed() {
				metrics.render_frame_times_set_max_len(window_id, max_len);
			}
		});

		ui.toggle_value(&mut is_histogram, "Histogram");

		match *is_histogram {
			true => {
				ui.horizontal(|ui| {
					ui.label("Time scale: ");
					egui::Slider::new(&mut *histogram_time_scale, 1.0..=1000.0)
						.logarithmic(true)
						.clamping(egui::SliderClamping::Always)
						.ui(ui);
				});
			},
			false => {
				ui.toggle_value(&mut stack_charts, "Stack charts");
			},
		}
	});

	let Some(render_frame_times) = render_frame_times.get(&window_id) else {
		return;
	};

	let mut charts = vec![];
	for duration_idx in 0..6 {
		let bars = match *is_histogram {
			true => {
				let mut buckets = HashMap::<_, usize>::new();
				for render_frame_time in render_frame_times {
					let render_frame_time =
						self::render_frame_time_non_cumulative(render_frame_time, duration_idx).as_millis_f64();
					#[expect(clippy::cast_sign_loss, reason = "Durations are positive")]
					let bucket_idx = (render_frame_time * *histogram_time_scale) as usize;

					*buckets.entry(bucket_idx).or_default() += 1;
				}

				buckets
					.into_iter()
					.map(|(bucket_idx, bucket)| {
						let width = 1.0 / *histogram_time_scale;
						let center = bucket_idx as f64 / *histogram_time_scale + width / 2.0;
						let height = *histogram_time_scale * bucket as f64 / render_frame_times.len() as f64;

						egui_plot::Bar::new(center, height).width(width)
					})
					.collect()
			},
			false => render_frame_times
				.iter()
				.enumerate()
				.map(|(frame_idx, render_frame_time)| {
					egui_plot::Bar::new(
						frame_idx as f64,
						self::render_frame_time_non_cumulative(render_frame_time, duration_idx).as_millis_f64(),
					)
					.width(1.0)
				})
				.collect(),
		};

		let mut chart = egui_plot::BarChart::new(self::render_frame_time_name(duration_idx), bars);
		if !*is_histogram && *stack_charts {
			chart = chart.stack_on(&charts.iter().collect::<Vec<_>>());
		}
		charts.push(chart);
	}

	let plot = egui_plot::Plot::new("Frame times")
		.legend(egui_plot::Legend::default())
		.clamp_grid(true);

	let plot = match *is_histogram {
		true => plot.x_axis_label("Time").y_axis_label("Occurrences (normalized)"),
		false => plot.x_axis_label("Frame").y_axis_label("Time"),
	};

	plot.show(ui, |plot_ui| {
		for chart in charts {
			plot_ui.bar_chart(chart);
		}
	});
}

/// Renders the window select and returns the selected one
pub fn render_window_select(ui: &mut egui::Ui, metrics: &Metrics) -> Option<WindowId> {
	let cur_window_id = super::get_data::<Option<WindowId>>(ui, "metrics-tab-cur-window");
	let mut cur_window_id = cur_window_id.lock();

	let windows = metrics.window_ids::<BTreeSet<_>>();

	// If we don't have a current window, use the first one available (if any)
	if cur_window_id.is_none() &&
		let Some(&window_id) = windows.first()
	{
		*cur_window_id = Some(window_id);
	}

	egui::ComboBox::from_id_salt("metrics-tab-window-selector")
		.selected_text(cur_window_id.map_or("None".to_owned(), |id| format!("{id:?}")))
		.show_ui(ui, |ui| {
			for window_id in windows {
				ui.selectable_value(&mut *cur_window_id, Some(window_id), format!("{window_id:?}"));
			}
		});

	*cur_window_id
}

/// Returns the duration with index `idx`
pub fn render_frame_time(frame_time: &RenderFrameTime, idx: usize) -> Duration {
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
pub fn render_frame_time_non_cumulative(frame_time: &RenderFrameTime, idx: usize) -> Duration {
	match idx {
		0 => self::render_frame_time(frame_time, 0),
		_ => self::render_frame_time(frame_time, idx) - self::render_frame_time(frame_time, idx - 1),
	}
}

/// Returns the name for a frame time duration
pub fn render_frame_time_name(idx: usize) -> &'static str {
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
