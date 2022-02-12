//! Utility

// Modules
mod display_wrapper;
mod scan_dir;
mod side_effect;
mod thread;

// Exports
pub use {
	display_wrapper::DisplayWrapper,
	scan_dir::visit_files_dir,
	side_effect::{extse, MightBlock, SideEffect, WithSideEffect},
	thread::ThreadSpawner,
};

// Imports
use {
	self::extse::ParkingLotMutexSe,
	anyhow::Context,
	image::DynamicImage,
	parking_lot::{Condvar, Mutex},
	std::{
		fs,
		future::Future,
		hash::{Hash, Hasher},
		path::Path,
		sync::{
			atomic::{self, AtomicBool},
			Arc,
		},
		task,
		time::{Duration, Instant},
	},
};

/// Measures how long it took to execute a function
pub fn measure<T>(f: impl FnOnce() -> T) -> (T, Duration) {
	let start_time = Instant::now();
	let value = f();
	let duration = Instant::now().saturating_duration_since(start_time);
	(value, duration)
}

pub macro measure_dbg {
	() => {
		::std::eprintln!("[{}:{}]", ::std::file!(), ::std::line!())
	},
	($value:expr $(,)?) => {
		match $crate::util::measure(|| $value) {
			(value, duration) => {
				::std::eprintln!("[{}:{}] {} took {:?}",
					::std::file!(), ::std::line!(), ::std::stringify!($value), duration);
				value
			}
		}
	},
	($($val:expr),+ $(,)?) => {
		($(::std::dbg!($val)),+,)
	}
}

/// Parses json from a file
pub fn parse_json_from_file<T: serde::de::DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, anyhow::Error> {
	// Open the file
	let file = fs::File::open(path).context("Unable to open file")?;

	// Then parse it
	serde_json::from_reader(file).context("Unable to parse file")
}

/// Hashes a value using `twox_hash`
pub fn _hash_of<T: ?Sized + Hash>(value: &T) -> u64 {
	let mut hasher = twox_hash::XxHash64::with_seed(0);
	value.hash(&mut hasher);
	hasher.finish()
}

/// Returns the image format string of an image (for logging)
pub fn image_format(image: &DynamicImage) -> &'static str {
	match image {
		DynamicImage::ImageLuma8(_) => "Luma8",
		DynamicImage::ImageLumaA8(_) => "LumaA8",
		DynamicImage::ImageRgb8(_) => "Rgb8",
		DynamicImage::ImageRgba8(_) => "Rgba8",
		DynamicImage::ImageBgr8(_) => "Bgr8",
		DynamicImage::ImageBgra8(_) => "Bgra8",
		DynamicImage::ImageLuma16(_) => "Luma16",
		DynamicImage::ImageLumaA16(_) => "LumaA16",
		DynamicImage::ImageRgb16(_) => "Rgb16",
		DynamicImage::ImageRgba16(_) => "Rgba16",
	}
}

/// Adapts a future into a thread to be run on it's own thread.
///
/// Will drop the future once `should_quit` becomes true.
// TODO: Use custom type to we can set `should_quit` to `true` and wake the waker simultaneously.
pub fn never_fut_thread_fn<'a, T, F>(should_quit: &'a AtomicBool, res: T, f: F) -> impl FnOnce() -> T + 'a
where
	T: 'a,
	F: Future<Output = !> + Send + 'a,
{
	move || {
		// TODO: Not allocate here
		let mut f = Box::pin(f);

		// Create the waker
		let signal = Arc::new(NeverFutSignal::new());
		let waker = task::Waker::from(Arc::clone(&signal));
		let mut ctx = task::Context::from_waker(&waker);

		// Then poll it until we should quit
		loop {
			match f.as_mut().poll(&mut ctx) {
				task::Poll::Ready(never) => never,
				task::Poll::Pending => match should_quit.load(atomic::Ordering::Relaxed) {
					true => break,
					false => signal.wait(),
				},
			}
		}

		res
	}
}

/// Signal for [`spawn_fut_never`]'s waker
struct NeverFutSignal {
	/// If the future should be polled
	should_poll: Mutex<bool>,

	/// Condvar for waiting
	cond_var: Condvar,
}

impl NeverFutSignal {
	fn new() -> Self {
		Self {
			should_poll: Mutex::new(true),
			cond_var:    Condvar::new(),
		}
	}

	/// Waits until the future should be polled
	pub fn wait(&self) {
		// Keep waiting until `should_poll` is true
		// DEADLOCK: Waker will set `should_poll` to true eventually.
		let mut should_poll = self.should_poll.lock_se().allow::<MightBlock>();
		while !*should_poll {
			self.cond_var.wait(&mut should_poll);
		}

		// Then set it to false so the waker may re-set it to true
		*should_poll = false;
	}
}

impl task::Wake for NeverFutSignal {
	fn wake(self: std::sync::Arc<Self>) {
		// Set that we should be polling
		// DEADLOCK: Mutex is only ever locked temporarily (as `wait`ing unlocks the mutex).
		let mut should_poll = self.should_poll.lock_se().allow::<MightBlock>();
		*should_poll = true;

		// Then notify the waiter
		let _ = self.cond_var.notify_one();
	}
}
