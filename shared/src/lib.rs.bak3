pub mod auth;
pub mod error;
pub mod models;
pub mod store;

#[cfg(test)]
pub mod tests;

// Test utilities - publicly exposed with test feature
#[cfg_attr(test, path = "test_utils/mod.rs")]
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
