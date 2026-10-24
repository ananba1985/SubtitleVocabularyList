//! Shared Tauri command boundary. Business rules remain in Application and Store.
pub(crate) mod commands;

/// The mobile WebView can finish loading before the host setup closure returns.
/// Keep readiness outside Application so its command is callable from first paint.
pub(crate) struct HostStartup {
    ready: tokio::sync::watch::Sender<bool>,
    #[cfg(all(windows, feature = "desktop"))]
    maintenance_started: std::sync::atomic::AtomicBool,
}

impl Default for HostStartup {
    fn default() -> Self {
        Self {
            ready: tokio::sync::watch::channel(false).0,
            #[cfg(all(windows, feature = "desktop"))]
            maintenance_started: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl HostStartup {
    pub(crate) fn ready(&self) { self.ready.send_replace(true); }

    #[cfg(all(windows, feature = "desktop"))]
    pub(crate) fn begin_maintenance(&self) -> bool {
        !self.maintenance_started.swap(true, std::sync::atomic::Ordering::AcqRel)
    }

    pub(crate) async fn wait(&self) -> Result<(), crate::error::AppError> {
        let mut state = self.ready.subscribe();
        loop {
            if *state.borrow_and_update() { return Ok(()); }
            state.changed().await.map_err(|_| crate::error::AppError::new(
                "initialization_failed", "手机本地词库初始化中断，请重新打开应用。",
            ))?;
        }
    }
}
