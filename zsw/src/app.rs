//! App

// Lints
// We need to share a lot of state and we can't couple it together in most cases
#![allow(clippy::too_many_arguments)]

// Modules
mod event_handler;
mod services;

// Imports
use {
	self::{event_handler::EventHandler, services::Services},
	crate::Args,
	anyhow::Context,
	pollster::FutureExt,
	std::sync::Arc,
	winit::platform::run_return::EventLoopExtRunReturn,
};

/// Runs the application
pub async fn run(args: Arc<Args>) -> Result<(), anyhow::Error> {
	// Create all services
	let (mut event_loop, services) = Services::new().await?;

	// Create the event handler
	let mut event_handler = EventHandler::new();

	// Spawn all futures
	let join_handle = services.spawn(&args);

	// Run the event loop until exit
	event_loop.run_return(|event, _, control_flow| {
		event_handler.handle_event(&*services, event, control_flow).block_on();
	});

	// Then join all tasks
	join_handle.await.context("Unable to join all tasks")?;

	Ok(())
}
