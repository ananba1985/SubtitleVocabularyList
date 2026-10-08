pub mod application;
pub mod corpus;
pub mod error;
pub mod known_targets;
pub mod lookup;
pub mod media;
pub mod ocr;
pub mod review_policy;
pub mod reviews;
pub mod site_connection;
pub mod store;
pub mod sync_data;
pub mod sync_merge;
#[cfg(all(test, windows, feature = "desktop"))]
mod sync_test_fixture;
#[cfg(test)]
mod sync_tests;
pub mod synchronization;
pub mod tasks;
pub mod vocabulary;

#[cfg(all(windows, feature = "desktop"))]
pub mod credentials;
#[cfg(all(windows, feature = "desktop"))]
mod desktop_capture;
#[cfg(all(windows, feature = "desktop"))]
mod desktop_ocr;
#[cfg(all(windows, feature = "desktop"))]
mod sync_http;
#[cfg(all(windows, feature = "desktop"))]
mod sync_transport;
#[cfg(all(windows, feature = "desktop"))]
pub mod windows_native;

#[cfg(feature = "desktop")]
mod desktop;

#[cfg(feature = "desktop")]
pub use desktop::run;
