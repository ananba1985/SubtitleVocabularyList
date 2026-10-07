pub mod application;
pub mod corpus;
pub mod error;
pub mod media;
pub mod ocr;
pub mod review_policy;
pub mod store;
pub mod tasks;
pub mod vocabulary;

#[cfg(all(windows, feature = "desktop"))]
mod desktop_capture;
#[cfg(all(windows, feature = "desktop"))]
mod desktop_ocr;
#[cfg(all(windows, feature = "desktop"))]
pub mod windows_native;

#[cfg(feature = "desktop")]
mod desktop;

#[cfg(feature = "desktop")]
pub use desktop::run;
