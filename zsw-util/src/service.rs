//! Services

/// Services bundle
pub trait ServicesBundle {
	/// Retrieves the service `Service`
	fn service<Service>(&self) -> &Service
	where
		Self: self::Services<Service>,
	{
		self.get()
	}
}

/// Services bundle that contains `Service`
pub trait Services<Service>: ServicesBundle {
	/// Retrieve `Service`
	fn get(&self) -> &Service;
}
