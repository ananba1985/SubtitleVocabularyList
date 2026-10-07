pub mod application;
pub mod corpus;
pub mod error;
pub mod media;
pub mod store;
pub mod tasks;
pub mod vocabulary;

#[cfg(feature = "desktop")]
mod desktop;

#[cfg(feature = "desktop")]
pub use desktop::run;
