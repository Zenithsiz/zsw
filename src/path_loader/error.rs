//! Errors


/// Error for [`PathLoader::new`](super::PathLoader::new)
#[derive(Debug, thiserror::Error)]
pub enum NewError {
	/// Unable to create filesystem watcher
	#[error("Unable to create filesystem watcher")]
	CreateFsWatcher(#[source] notify::Error),

	/// Unable to start watching filesystem directory
	#[error("Unable to start watching filesystem directory")]
	WatchFilesystemDir(#[source] notify::Error),

	/// Unable to create loader thread
	#[error("Unable to create loader thread")]
	CreateLoaderThread(#[source] std::io::Error),

	/// Unable to create distributer thread
	#[error("Unable to create distributer thread")]
	CreateDistributerThread(#[source] std::io::Error),
}
