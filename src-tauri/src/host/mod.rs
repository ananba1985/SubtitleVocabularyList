//! Shared Tauri command boundary. Business rules remain in Application and Store.
pub(crate) mod commands;

/// The mobile WebView can finish loading before the host setup closure returns.
/// Keep readiness outside Application so its command is callable from first paint.
pub(crate) struct HostStartup(tokio::sync::watch::Sender<bool>);

impl Default for HostStartup {
    fn default() -> Self { Self(tokio::sync::watch::channel(false).0) }
}

impl HostStartup {
    pub(crate) fn ready(&self) { self.0.send_replace(true); }

    pub(crate) async fn wait(&self) -> Result<(), crate::error::AppError> {
        let mut state = self.0.subscribe();
        loop {
            if *state.borrow_and_update() { return Ok(()); }
            state.changed().await.map_err(|_| crate::error::AppError::new(
                "initialization_failed", "手机本地词库初始化中断，请重新打开应用。",
            ))?;
        }
    }
}
