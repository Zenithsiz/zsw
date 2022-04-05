//! Services

/// Services bundle
pub trait ServicesBundle {
	/// Retrieves the service `Service`
	fn service<Service>(&self) -> &Service
	where
		Self: self::ServicesContains<Service>,
	{
		self.get()
	}
}

/// Services bundle that contains `Service`
pub trait ServicesContains<Service> {
	/// Retrieve `Service`
	fn get(&self) -> &Service;
}
