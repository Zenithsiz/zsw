//! Resources

// Imports
use {
	crate::AppError,
	app_error::{Context, app_error},
	core::ops,
	serde::de::DeserializeOwned,
	std::{collections::HashMap, ffi::OsStr, fs, hash::Hash, path::Path},
};

/// Resources
#[derive(Debug)]
pub struct Resources<N, V> {
	/// Loaded values
	values: HashMap<N, V>,
}

impl<N, V> Resources<N, V> {
	/// Loads resources from a directory.
	pub fn new(root: &Path) -> Result<Self, AppError>
	where
		N: Eq + Hash + Clone + From<String> + Send + 'static,
		V: DeserializeOwned + Send + Sync + 'static,
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
			let value = toml::from_str(&toml).with_context(|| format!("Unable to parse file {entry_path:?}"))?;

			_ = values.insert(name, value);
		}

		Ok(Self { values })
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


impl<N, V> ops::Index<&N> for Resources<N, V>
where
	N: Eq + Hash,
{
	type Output = V;

	fn index(&self, idx: &N) -> &Self::Output {
		&self.values[idx]
	}
}
