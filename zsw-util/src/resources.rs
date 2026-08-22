//! Resources

// Imports
use {
	crate::AppError,
	app_error::{Context, app_error},
	core::marker::PhantomData,
	serde::de::DeserializeOwned,
	std::{collections::HashMap, ffi::OsStr, fs, hash::Hash, path::Path, sync::Arc},
};

/// Resources
#[derive(Debug)]
pub struct Resources<N, V, S> {
	/// Loaded values
	values: HashMap<N, V>,

	/// Phantom for the serialized type
	_phantom: PhantomData<fn() -> S>,
}

impl<N, V, S> Resources<N, V, S> {
	/// Loads resources from a directory.
	pub fn new(root: &Path) -> Result<Self, AppError>
	where
		N: Eq + Hash + Clone + From<String> + Send + 'static,
		V: FromSerialized<N, S> + Send + Sync + 'static,
		S: DeserializeOwned + 'static,
	{
		fs::create_dir_all(root).context("Unable to create root directory")?;
		let dir = fs::read_dir(root).context("Unable to read directory")?;

		let mut values = HashMap::new();
		for entry in dir {
			// Ignore directories and non `.toml` files
			let entry = entry.context("Unable to get entry")?;
			let entry_path = entry.path();
			if entry.file_type().context("Unable to get entry metadata")?.is_dir() ||
				entry_path.extension().and_then(OsStr::to_str) != Some("toml")
			{
				continue;
			}

			// Then get the name from the file
			let name = entry_path
				.file_stem()
				.context("Entry path had no file stem")?
				.to_os_string()
				.into_string()
				.map(N::from)
				.map_err(|file_name| app_error!("Entry name was non-utf8: {file_name:?}"))?;

			// Try to read the file
			let toml =
				fs::read_to_string(&entry_path).with_context(|| format!("Unable to read file {entry_path:?}"))?;

			// And parse it
			let value = toml::from_str::<S>(&toml).with_context(|| format!("Unable to parse file {entry_path:?}"))?;
			let value = V::from_serialized(name.clone(), value);

			_ = values.insert(name, value);
		}

		Ok(Self {
			values,
			_phantom: PhantomData,
		})
	}

	/// Gets a value by name
	pub fn get(&self, name: &N) -> Option<&V>
	where
		N: Eq + Hash,
	{
		self.values.get(name)
	}

	/// Returns an iterator over all names
	pub fn names(&self) -> impl Iterator<Item = &N> {
		self.values.keys()
	}

	/// Returns an iterator over all values
	pub fn iter(&self) -> impl Iterator<Item = (&N, &V)> {
		self.values.iter()
	}
}

/// Types which may be converted from their serialized variant
pub trait FromSerialized<N, S> {
	/// Converts this type from it's serialized form
	fn from_serialized(name: N, value: S) -> Self;
}

impl<N, S, T> FromSerialized<N, S> for Arc<T>
where
	T: FromSerialized<N, S>,
{
	fn from_serialized(name: N, value: S) -> Self {
		Self::new(T::from_serialized(name, value))
	}
}
