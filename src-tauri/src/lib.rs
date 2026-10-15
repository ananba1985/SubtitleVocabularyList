pub mod application;
pub mod assessment;
pub mod assessment_bank;
mod assessment_sampling;
#[cfg(test)]
mod assessment_tests;
#[cfg(test)]
mod assessment_bank_tests;
mod base_dictionary;
mod batch_explanations;
pub mod corpus;
pub mod desktop_layout;
pub mod error;
pub mod explanations;
pub mod known_targets;
pub mod lookup;
pub mod media;
mod model_gate;
pub mod ocr;
pub mod practice;
pub mod preparation;
pub mod providers;
pub mod queries;
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
mod app_paths;
#[cfg(all(windows, feature = "desktop"))]
mod desktop_capture;
#[cfg(all(windows, feature = "desktop"))]
mod desktop_ocr;
#[cfg(all(windows, feature = "desktop"))]
mod desktop_query;
#[cfg(all(windows, feature = "desktop"))]
mod desktop_replacement;
#[cfg(all(windows, feature = "desktop"))]
mod replacement_target;
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
