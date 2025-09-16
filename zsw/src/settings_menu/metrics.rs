//! Metrics tab

// Imports
use {
	crate::{
		metrics::{FrameTimes, Metrics, RenderFrameTime},
		window::WindowMonitorNames,
	},
	core::time::Duration,
	egui::Widget,
	std::collections::{BTreeSet, HashMap},
	winit::window::WindowId,
	zsw_util::TokioTaskBlockOn,
};

/// Draws the metrics tab
pub fn draw_metrics_tab(ui: &mut egui::Ui, metrics: &Metrics, window_monitor_names: &WindowMonitorNames) {
	// Get the window, otherwise we have nothing to render
	let Some(window_id) = self::draw_window_select(ui, metrics, window_monitor_names) else {
		ui.weak("No window selected");
		return;
	};

	self::draw_render_frame_times(ui, &mut metrics.render_frame_times(window_id).block_on());
}

/// Draws the render frame times
fn draw_render_frame_times(ui: &mut egui::Ui, render_frame_times: &mut FrameTimes<RenderFrameTime>) {
	let settings = self::draw_frame_time_settings(ui, render_frame_times);

	let mut charts = vec![];
	for duration_idx in 0..6 {
		let chart = self::add_frame_time_chart(
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
		true => plot.x_axis_label("Time (s)").y_axis_label("Occurrences (normalized)"),
		false => plot.x_axis_label("Frame").y_axis_label("Time (s)"),
	};

	plot.show(ui, |plot_ui| {
		for chart in charts {
			plot_ui.bar_chart(chart);
		}
	});
}

struct RenderFrameTimeSettings {
	is_histogram:         bool,
	histogram_time_scale: f64,
	stack_charts:         bool,
}

/// Draws a frame time's settings
fn draw_frame_time_settings<T>(ui: &mut egui::Ui, frame_times: &mut FrameTimes<T>) -> RenderFrameTimeSettings {
	// TODO: Turn this into some enum between histogram / time
	let is_histogram = super::get_data::<bool>(ui, "metrics-tab-histogram");
	let mut is_histogram = is_histogram.lock();

	let histogram_time_scale = super::get_data_with_default::<f64>(ui, "metrics-tab-histogram-time-scale", || 20.0);
	let mut histogram_time_scale = histogram_time_scale.lock();

	let stack_charts = super::get_data_with_default::<bool>(ui, "metrics-tab-chart-stacks", || true);
	let mut stack_charts = stack_charts.lock();

	ui.horizontal(|ui| {
		let mut is_paused = frame_times.is_paused();
		if ui.toggle_value(&mut is_paused, "Pause").changed() {
			frame_times.pause(is_paused);
		}

		let mut max_len = frame_times.max_len();
		ui.horizontal(|ui| {
			ui.label("Maximum frames: ");
			if egui::Slider::new(&mut max_len, 0..=60 * 100).ui(ui).changed() {
				frame_times.set_max_len(max_len);
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

	RenderFrameTimeSettings {
		is_histogram:         *is_histogram,
		histogram_time_scale: *histogram_time_scale,
		stack_charts:         *stack_charts,
	}
}

/// Creates a chart of frame times
fn add_frame_time_chart<T>(
	render_frame_times: &FrameTimes<T>,
	is_histogram: bool,
	histogram_time_scale: f64,
	stack_charts: bool,
	prev_charts: &[egui_plot::BarChart],
	chart_name: &'static str,
	get_duration: impl Fn(&T) -> Duration,
) -> egui_plot::BarChart {
	let bars = match is_histogram {
		true => {
			let mut buckets = HashMap::<_, usize>::new();
			for render_frame_time in render_frame_times.iter() {
				let render_frame_time = get_duration(render_frame_time).as_millis_f64();
				#[expect(clippy::cast_sign_loss, reason = "Durations are positive")]
				let bucket_idx = (render_frame_time * histogram_time_scale) as usize;

				*buckets.entry(bucket_idx).or_default() += 1;
			}

			buckets
				.into_iter()
				.map(|(bucket_idx, bucket)| {
					let width = 1.0 / histogram_time_scale;
					let center = bucket_idx as f64 / histogram_time_scale + width / 2.0;
					let height = histogram_time_scale * bucket as f64 / render_frame_times.len() as f64;

					egui_plot::Bar::new(center, height).width(width)
				})
				.collect()
		},
		false => render_frame_times
			.iter()
			.enumerate()
			.map(|(frame_idx, render_frame_time)| {
				egui_plot::Bar::new(frame_idx as f64, get_duration(render_frame_time).as_millis_f64()).width(1.0)
			})
			.collect(),
	};

	let mut chart = egui_plot::BarChart::new(chart_name, bars);
	if !is_histogram && stack_charts {
		chart = chart.stack_on(&prev_charts.iter().collect::<Vec<_>>());
	}
	chart
}

/// Draws the window select and returns the selected one
fn draw_window_select(
	ui: &mut egui::Ui,
	metrics: &Metrics,
	window_monitor_names: &WindowMonitorNames,
) -> Option<WindowId> {
	let cur_window_id = super::get_data::<Option<WindowId>>(ui, "metrics-tab-cur-window");
	let mut cur_window_id = cur_window_id.lock();

	let window_name = |window_id| {
		window_monitor_names
			.get(window_id)
			.unwrap_or_else(|| format!("{window_id:?}"))
	};

	let windows = metrics.window_ids::<BTreeSet<_>>().block_on();

	// If we don't have a current window, use the first one available (if any)
	if cur_window_id.is_none() &&
		let Some(&window_id) = windows.first()
	{
		*cur_window_id = Some(window_id);
	}

	egui::ComboBox::from_id_salt("metrics-tab-window-selector")
		.selected_text(cur_window_id.map_or("None".to_owned(), window_name))
		.show_ui(ui, |ui| {
			for window_id in windows {
				ui.selectable_value(&mut *cur_window_id, Some(window_id), window_name(window_id));
			}
		});

	*cur_window_id
}
