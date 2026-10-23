pub mod application;
pub mod assessment;
pub mod assessment_bank;
pub mod assessment_reports;
pub mod assessment_sync;
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
mod episode_sharing;
pub mod known_targets;
#[cfg(any(feature = "app-host", test))]
mod ipc_dto;
#[cfg(feature = "ipc-types")]
mod ipc_export;
pub mod lookup;
mod local_profile;
pub mod media;
mod model_gate;
pub mod ocr;
pub mod online_subtitles;
pub mod practice;
pub mod preparation;
pub mod providers;
pub mod queries;
mod query_stream;
pub mod review_policy;
pub mod reviews;
pub mod speech;
mod clip_audio;
mod mobile_video;
mod mobile_learning;
mod mobile_capture;
#[cfg(all(target_os = "android", feature = "mobile"))]
mod android_ocr;
pub mod site_connection;
#[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
mod device_sync;
pub mod store;
pub mod sync_data;
mod sync_delta;
mod sync_limits;
pub mod sync_merge;
#[cfg(all(test, windows, feature = "desktop"))]
mod sync_test_fixture;
#[cfg(test)]
mod sync_tests;
pub mod synchronization;
pub mod tasks;
pub mod vocabulary;
pub mod watch;
pub mod playback;
pub mod related_videos;
pub mod subtitles;

#[cfg(all(windows, feature = "desktop"))]
mod player;

#[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
pub mod credentials;
#[cfg(all(windows, feature = "desktop"))]
mod app_paths;
#[cfg(all(windows, feature = "desktop"))]
mod autostart;
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
#[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
mod sync_http;
#[cfg(any(all(windows, feature = "desktop"), all(target_os = "android", feature = "mobile")))]
mod sync_transport;
#[cfg(all(windows, feature = "desktop"))]
pub mod windows_native;

#[cfg(feature = "app-host")]
mod host;

#[cfg(all(target_os = "android", feature = "mobile"))]
mod mobile;
#[cfg(all(target_os = "android", feature = "mobile"))]
mod mobile_platform;

#[cfg(all(target_os = "android", feature = "mobile"))]
pub use mobile::run;

#[cfg(all(windows, feature = "desktop"))]
mod desktop;

#[cfg(all(windows, feature = "desktop"))]
pub use desktop::run;

#[cfg(feature = "ipc-types")]
pub fn export_ipc_types() -> String { desktop::export_ipc_types() }
