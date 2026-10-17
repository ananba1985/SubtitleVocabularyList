//! Current negotiated sync capacity. Limits never justify truncating learning facts.
use crate::error::AppError;

pub(crate) const MAX_PACKET_BYTES: usize = 8_388_608;
// Reserve 4 KiB for the response/request envelope, matching the deployed site.
pub(crate) const MAX_BUNDLE_BYTES: usize = 8_384_512;
pub(crate) const MAX_AUDIO_BYTES: usize = 20_971_520;

pub(crate) fn capacity_error(kind: &str, actual: usize, limit: usize) -> AppError {
    AppError::new(
        "capacity_exceeded",
        format!(
            "{kind}为 {actual} 字节（{:.2} MiB），超过本版本的 {limit} 字节（{:.2} MiB）同步上限。该资料未提交，完整例句、原声和学习历史保留，不需要删除历史。",
            actual as f64 / 1_048_576.0,
            limit as f64 / 1_048_576.0
        ),
    )
}

pub(crate) fn check_push_size(
    kind: &str,
    bytes: usize,
    change: &str,
    revision: i64,
) -> Result<(), AppError> {
    if bytes > MAX_PACKET_BYTES {
        return Err(capacity_error(kind, bytes, MAX_PACKET_BYTES));
    }
    // Both versioned push contracts have the same envelope. Serialize only its
    // small metadata; the complete bundle has already been encoded exactly once.
    let envelope = serde_json::to_vec(
        &serde_json::json!({"changeId":change,"baseRevision":revision,"bundle":null}),
    )?
    .len()
        - 4;
    let bundle_bytes = bytes
        .checked_sub(envelope)
        .ok_or_else(|| AppError::new("internal_error", "同步资料信封大小无效。"))?;
    if bundle_bytes > MAX_BUNDLE_BYTES {
        return Err(capacity_error(kind, bundle_bytes, MAX_BUNDLE_BYTES));
    }
    Ok(())
}
