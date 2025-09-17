//! Frame times metrics

// Modules
pub mod render;
pub mod render_panels;

// Imports
use {
	crate::{menu, metrics::FrameTimes},
	core::time::Duration,
	egui::{Widget, style},
	std::collections::{HashMap, HashSet},
};

/// Draws a frame time's plot
fn draw_plot<T, I, D>(ui: &mut egui::Ui, frame_times: &FrameTimes<T>, display: &FrameTimesDisplay, duration_idxs: I)
where
	I: IntoIterator<Item = D> + Clone,
	D: DurationIdx<T>,
{
	let legend = egui_plot::Legend::default().follow_insertion_order(true);

	let plot = egui_plot::Plot::new("Render frame times")
		.legend(legend)
		.clamp_grid(true);

	let plot = match display {
		FrameTimesDisplay::TimeGraph { .. } => plot.x_axis_label("Frame").y_axis_label("Time (ms)"),
		FrameTimesDisplay::Histogram { .. } => plot.x_axis_label("Time (ms)").y_axis_label("Occurrences (normalized)"),
	};

	let disabled_duration_idxs =
		menu::get_data::<HashSet<String>>(ui, format!("metrics-tab-disabled-durations-{frame_times:p}"));
	let mut disabled_duration_idxs = disabled_duration_idxs.lock();

	egui::CollapsingHeader::new("Enable").show(ui, |ui| {
		ui.style_mut().spacing.scroll = style::ScrollStyle::solid();
		egui::ScrollArea::horizontal().show(ui, |ui| {
			ui.horizontal(|ui| {
				for duration_idx in duration_idxs.clone() {
					let name = duration_idx.name();
					let mut is_enabled = !disabled_duration_idxs.contains(&name);
					if ui.checkbox(&mut is_enabled, &name).clicked() {
						match is_enabled {
							true => _ = disabled_duration_idxs.remove(&name),
							false => _ = disabled_duration_idxs.insert(name),
						}
					}
				}
			});
		});
	});

	plot.show(ui, |plot_ui| {
		let mut prev_heights = vec![0.0; frame_times.len()];
		let charts = duration_idxs.into_iter().filter_map(move |duration_idx| {
			if disabled_duration_idxs.contains(&duration_idx.name()) {
				return None;
			}

			Some(self::create_frame_time_chart(
				frame_times,
				display,
				&mut prev_heights,
				&duration_idx,
			))
		});

		for chart in charts {
			plot_ui.bar_chart(chart);
		}
	});
}

/// Display
#[derive(Clone, Copy, Debug)]
enum FrameTimesDisplay {
	/// Time graph
	TimeGraph { stack_charts: bool },

	/// Histogram
	Histogram { time_scale: f64 },
}

/// Draws a the frame time display settings
fn draw_display_settings<T>(ui: &mut egui::Ui, frame_times: &mut FrameTimes<T>) -> FrameTimesDisplay {
	// Note: We don't store a `FrameTimesDisplay` directly so we can keep track of
	//       each field separately in it's own instance.
	#[derive(PartialEq, Clone, Copy, Default, Debug)]
	#[derive(derive_more::Display, strum::EnumIter)]
	enum FrameTimesDisplayKind {
		#[display("Time graph")]
		#[default]
		TimeGraph,
		#[display("Histogram")]
		Histogram,
	}
	#[derive(Clone, Copy, Default, Debug)]
	struct FrameTimesDisplayData {
		time_graph: FrameTimesDisplayTimeGraphData,
		histogram:  FrameTimesDisplayHistogramData,
	}
	#[derive(Clone, Copy, Default, Debug)]
	struct FrameTimesDisplayTimeGraphData {
		stack_charts: bool = true,
	}
	#[derive(Clone, Copy, Default, Debug)]
	struct FrameTimesDisplayHistogramData {
		time_scale:    f64 = 10.0,
	}

	let cur_kind = menu::get_data::<FrameTimesDisplayKind>(ui, "metrics-tab-display-kind");
	let mut cur_kind = cur_kind.lock();

	let cur_data = menu::get_data::<FrameTimesDisplayData>(ui, "metrics-tab-display-data");
	let mut cur_data = cur_data.lock();

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
	});

	ui.horizontal(|ui| {
		ui.label("Display: ");

		egui::ComboBox::from_id_salt("metrics-tab-display-selector")
			.selected_text(cur_kind.to_string())
			.show_ui(ui, |ui| {
				let kin = [FrameTimesDisplayKind::TimeGraph, FrameTimesDisplayKind::Histogram];
				for kind in kin {
					ui.selectable_value(&mut *cur_kind, kind, kind.to_string());
				}
			});

		match &mut *cur_kind {
			FrameTimesDisplayKind::TimeGraph => {
				ui.toggle_value(&mut cur_data.time_graph.stack_charts, "Stack charts");
			},
			FrameTimesDisplayKind::Histogram => {
				ui.horizontal(|ui| {
					ui.label("Time scale: ");
					egui::Slider::new(&mut cur_data.histogram.time_scale, 1.0..=1000.0)
						.logarithmic(true)
						.clamping(egui::SliderClamping::Always)
						.ui(ui);
				});
			},
		}
	});

	match *cur_kind {
		FrameTimesDisplayKind::TimeGraph => FrameTimesDisplay::TimeGraph {
			stack_charts: cur_data.time_graph.stack_charts,
		},
		FrameTimesDisplayKind::Histogram => FrameTimesDisplay::Histogram {
			time_scale: cur_data.histogram.time_scale,
		},
	}
}

/// Creates a chart of frame times
fn create_frame_time_chart<T, D>(
	frame_times: &FrameTimes<T>,
	display: &FrameTimesDisplay,
	prev_heights: &mut [f64],
	duration_idx: &D,
) -> egui_plot::BarChart
where
	D: DurationIdx<T>,
{
	let bars = match display {
		FrameTimesDisplay::TimeGraph { stack_charts } => frame_times
			.iter()
			.enumerate()
			.filter_map(|(frame_idx, frame_time)| {
				let height = duration_idx.duration_of(frame_time)?.as_millis_f64();
				let mut bar = egui_plot::Bar::new(frame_idx as f64, height).width(1.0);

				if *stack_charts {
					bar = bar.base_offset(prev_heights[frame_idx]);
					prev_heights[frame_idx] += height;
				}

				Some(bar)
			})
			.collect(),
		FrameTimesDisplay::Histogram { time_scale } => {
			let mut buckets: HashMap<usize, usize> = HashMap::<_, usize>::new();
			for frame_time in frame_times.iter() {
				let Some(duration) = duration_idx.duration_of(frame_time) else {
					continue;
				};
				#[expect(clippy::cast_sign_loss, reason = "Durations are positive")]
				let bucket_idx = (duration.as_millis_f64() * time_scale) as usize;

				*buckets.entry(bucket_idx).or_default() += 1;
			}

			buckets
				.into_iter()
				.map(|(bucket_idx, bucket)| {
					let width = 1.0 / time_scale;
					let center = bucket_idx as f64 / time_scale + width / 2.0;
					let height = time_scale * bucket as f64 / frame_times.len() as f64;

					egui_plot::Bar::new(center, height).width(width)
				})
				.collect()
		},
	};

	egui_plot::BarChart::new(duration_idx.name(), bars)
}

/// Duration index
pub trait DurationIdx<T> {
	/// Returns the name of this index
	fn name(&self) -> String;

	/// Returns the duration relative to this index
	fn duration_of(&self, frame_time: &T) -> Option<Duration>;
}
