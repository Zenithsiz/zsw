//! Duration display

// Imports
use {
	crate::AppError,
	app_error::Context,
	core::{fmt, str::FromStr, time::Duration},
};

/// Duration
#[derive(Clone, Copy, Debug)]
#[derive(serde_with::SerializeDisplay)]
#[derive(serde_with::DeserializeFromStr)]
pub struct DurationDisplay(pub Duration);

impl FromStr for DurationDisplay {
	type Err = AppError;

	fn from_str(mut s: &str) -> Result<Self, Self::Err> {
		let mut time = Duration::ZERO;

		// Remove any hours from the time
		if let Some((hours, rest)) = s.split_once('h') {
			let hours = hours
				.parse::<u64>()
				.with_context(|| format!("Expected an integer before `h`, found {hours:?}"))?;
			time += Duration::from_hours(hours);
			s = rest;
		}

		// Remove any minutes from the time
		if let Some((mins, rest)) = s.split_once('m') {
			let mins = mins
				.parse::<u64>()
				.with_context(|| format!("Expected an integer before `m`, found {mins:?}"))?;
			time += Duration::from_mins(mins);
			s = rest;
		}

		// Then remove any trailing `s` the user might have added
		let secs = s.strip_suffix('s').unwrap_or(s);

		// And parse the rest as seconds (may be empty)
		let secs = match secs {
			"" => 0.0,
			_ => secs
				.parse::<f64>()
				.with_context(|| format!("Expected a number of seconds, found {secs:?}"))?,
		};
		time += Duration::from_secs_f64(secs);

		Ok(Self(time))
	}
}

impl fmt::Display for DurationDisplay {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let hours = self.0.as_secs() / 3600;
		if hours != 0 {
			write!(f, "{hours}h")?;
		}

		let mins = (self.0 - Duration::from_hours(hours)).as_secs() / 60;
		if mins != 0 {
			write!(f, "{mins}m")?;
		}

		let secs = (self.0 - Duration::from_hours(hours) - Duration::from_mins(mins)).as_secs_f64();
		if secs != 0.0 || (hours == 0 && mins == 0) {
			// TODO: Find some other way of having variable precision (up to millisecond)
			let mut secs = format!("{secs:.3}");
			while secs.ends_with('0') {
				_ = secs.pop();
			}
			if secs.ends_with('.') {
				_ = secs.pop();
			}
			secs.push('s');
			f.pad(&secs)?;
		}

		Ok(())
	}
}
