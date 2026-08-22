//! Resources

// Imports
use {
	crate::AppError,
	app_error::{Context, app_error},
	core::marker::PhantomData,
	serde::{Serialize, de::DeserializeOwned},
	std::{
		collections::HashMap,
		ffi::OsStr,
		fs,
		hash::Hash,
		path::PathBuf,
		sync::{
			Arc,
			OnceLock,
			nonpoison::{Mutex, RwLock},
		},
	},
};

/// Resource storage
type ResourceStorage<V> = Arc<OnceLock<Arc<RwLock<V>>>>;

/// Resources
#[derive(Debug)]
pub struct Resources<N, V, S> {
	/// Profiles directory
	root: PathBuf,

	/// Loaded values
	// TODO: Limit the size of this?
	values: Mutex<HashMap<N, ResourceStorage<V>>>,

	/// Phantom for the serialized type
	_phantom: PhantomData<fn() -> S>,
}

impl<N, V, S> Resources<N, V, S> {
	/// Loads resources from a directory.
	pub fn new(root: PathBuf) -> Result<Self, AppError> {
		fs::create_dir_all(&root).context("Unable to create root directory")?;

		Ok(Self {
			root,
			values: Mutex::new(HashMap::new()),
			_phantom: PhantomData,
		})
	}

	/// Loads all resources from the root directory
	pub fn load_all(self: &Arc<Self>) -> Result<(), AppError>
	where
		N: Eq + Hash + Clone + From<String> + AsRef<str> + Send + 'static,
		V: FromSerialized<N, S> + Send + Sync + 'static,
		S: DeserializeOwned + 'static,
	{
		let dir = fs::read_dir(&self.root).context("Unable to read directory")?;

		for entry in dir {
			// Ignore directories and non `.toml` files
			let entry = entry.context("Unable to get entry")?;
			let entry_path = entry.path();
			if entry.file_type().context("Unable to get entry metadata")?.is_dir() ||
				entry_path.extension().and_then(OsStr::to_str) != Some("toml")
			{
				return Ok(());
			}

			// Then get the name from the file
			let name = entry_path
				.file_stem()
				.context("Entry path had no file stem")?
				.to_os_string()
				.into_string()
				.map(N::from)
				.map_err(|file_name| app_error!("Entry name was non-utf8: {file_name:?}"))?;

			self.load(name)
				.map(|_| ())
				.with_context(|| format!("Unable to load file {entry_path:?}"))?;
		}

		Ok(())
	}

	/// Adds a new value
	pub fn add(&self, name: N, value: V) -> Arc<RwLock<V>>
	where
		N: Eq + Hash,
	{
		let value = Arc::new(RwLock::new(value));
		_ = self
			.values
			.lock()
			.insert(name, Arc::new(OnceLock::new_init(Arc::clone(&value))));

		value
	}

	/// Loads a value by name
	pub fn load(&self, name: N) -> Result<Arc<RwLock<V>>, AppError>
	where
		N: Eq + Hash + Clone + AsRef<str>,
		V: FromSerialized<N, S>,
		S: DeserializeOwned,
	{
		let entry = Arc::clone(
			self.values
				.lock()
				.entry(name.clone())
				.or_insert_with(|| Arc::new(OnceLock::new())),
		);

		entry
			.get_or_try_init(|| {
				// Try to read the file
				let path = self.path_of(&name);
				let toml = fs::read_to_string(path).context("Unable to open file")?;

				// And parse it
				let value = toml::from_str::<S>(&toml).context("Unable to parse value")?;
				let value = V::from_serialized(name, value);

				Ok(Arc::new(RwLock::new(value)))
			})
			.map(Arc::clone)
	}

	/// Saves a value by name
	pub fn save(&self, name: &N) -> Result<(), AppError>
	where
		N: Eq + Hash + AsRef<str>,
		V: ToSerialized<N, S>,
		S: Serialize,
	{
		let value_path = self.path_of(name);

		let value = {
			let values = self.values.lock();

			let value = values
				.get(name)
				.context("Unknown value name")?
				.get()
				.context("Value is still initializing")?;

			Arc::clone(value)
		};
		let value = value.read();
		let value = value.to_serialized(name);

		let value = toml::to_string_pretty(&value).context("Unable to serialize value")?;
		fs::write(&value_path, &value).context("Unable to write value")?;

		Ok(())
	}

	/// Returns all values
	pub fn get_all(&self) -> Vec<Arc<RwLock<V>>> {
		self.values
			.lock()
			.values()
			.filter_map(|value| value.get().map(Arc::clone))
			.collect()
	}

	/// Returns a value's path
	pub fn path_of(&self, name: &N) -> PathBuf
	where
		N: AsRef<str>,
	{
		self.root.join(name.as_ref()).with_added_extension("toml")
	}
}

/// Types which may be converted from their serialized variant
pub trait FromSerialized<N, S> {
	/// Converts this type from it's serialized form
	fn from_serialized(name: N, value: S) -> Self;
}

/// Types which may be converted into their serialized variant
pub trait ToSerialized<N, S> {
	/// Converts this type into it's serialized form
	fn to_serialized(&self, name: &N) -> S;
}
