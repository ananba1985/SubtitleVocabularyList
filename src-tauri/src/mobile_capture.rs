use crate::{
    application::Application,
    error::AppError,
    mobile_video::{self, MediaRef},
    ocr::PixelRect,
    store::Store,
    tasks::TaskHandle,
    watch::{WatchClipRange, WatchContext},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct IncomingInput {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub uri: String,
    pub title: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct FrameImage {
    pub uri: String,
    pub session_id: String,
    pub media_revision: u64,
    pub position_ms: u64,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub audio_track_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CaptureImage {
    pub id: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub origin: String,
    pub frame: Option<FrameImage>,
}
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OcrText {
    pub image_id: String,
    pub text: String,
}
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct FrameDraft {
    pub text: String,
    pub kind: String,
    pub context: String,
    pub meaning: String,
    pub source_id: String,
    pub source_title: String,
    pub location_key: String,
    pub example: Option<crate::corpus::CandidateExample>,
}

pub(crate) fn incoming() -> Result<Vec<IncomingInput>, AppError> {
    #[derive(Deserialize)]
    struct Inputs {
        inputs: Vec<IncomingInput>,
    }
    Ok(mobile_video::invoke::<Inputs>("incomingInputs", serde_json::json!({}))?.inputs)
}
pub(crate) fn ack(id: &str) -> Result<(), AppError> {
    mobile_video::invoke::<serde_json::Value>("incomingAck", serde_json::json!({"id":id}))?;
    Ok(())
}
fn image_key(id: &str) -> Result<String, AppError> {
    uuid::Uuid::parse_str(id).map_err(|_| AppError::new("invalid_input", "图片标识无效。"))?;
    Ok(format!("android.image:{id}"))
}
pub(crate) fn image(store: &Store, id: &str) -> Result<CaptureImage, AppError> {
    let image: CaptureImage = mobile_video::setting(store, &image_key(id)?)?
        .ok_or_else(|| AppError::new("resource_missing", "图片已不可用，请重新选择。"))?;
    let path = std::path::Path::new(&image.path)
        .canonicalize()
        .map_err(|_| AppError::new("resource_missing", "图片已不可用，请重新选择。"))?;
    if !path.starts_with(store.root().join("jobs").canonicalize()?) {
        return Err(AppError::new("invalid_input", "图片位置无效。"));
    }
    Ok(image)
}
pub(crate) fn prepare(store: &Store, id: String, uri: String) -> Result<CaptureImage, AppError> {
    if let Ok(existing) = image(store, &id) {
        return Ok(existing);
    }
    image_key(&id)?;
    let image: CaptureImage =
        mobile_video::invoke("prepareImage", serde_json::json!({"id":id,"uri":uri}))?;
    mobile_video::save(store, &image_key(&image.id)?, &image)?;
    Ok(image)
}
pub(crate) fn pick(store: &Store) -> Result<CaptureImage, AppError> {
    #[derive(Deserialize)]
    struct Picked {
        files: Vec<MediaRef>,
    }
    let picked: Picked =
        mobile_video::invoke("pickDocuments", serde_json::json!({"kind":"image"}))?;
    let file = picked
        .files
        .into_iter()
        .next()
        .ok_or_else(|| AppError::new("cancelled", "已取消选择图片。"))?;
    prepare(store, uuid::Uuid::new_v4().to_string(), file.uri)
}
pub(crate) fn frame(store: &Store) -> Result<CaptureImage, AppError> {
    let mut image: CaptureImage = mobile_video::invoke("videoFrame", serde_json::json!({}))?;
    let state = mobile_video::state(store)?;
    let captured = image
        .frame
        .as_mut()
        .ok_or_else(|| AppError::new("invalid_data", "暂停帧缺少来源。"))?;
    let study = mobile_video::study(store, &captured.uri)?
        .ok_or_else(|| AppError::new("resource_missing", "请等待视频指纹准备完成后截图。"))?;
    if state.session_id != captured.session_id || state.media_revision != captured.media_revision {
        return Err(AppError::new("conflict", "视频已切换，请重新截图。"));
    }
    captured.source_id = study.source_id;
    captured.sha256 = study.sha256;
    captured.title = store
        .playback_state()?
        .playlist
        .iter()
        .find(|i| i.path == captured.uri)
        .map(|i| i.title.clone())
        .unwrap_or_else(|| "视频暂停帧".into());
    captured.audio_track_id = state
        .tracks
        .iter()
        .find(|t| t.kind == "audio" && t.selected)
        .map(|t| t.format_id.clone());
    mobile_video::save(store, &image_key(&image.id)?, &image)?;
    Ok(image)
}

fn frame_context(
    store: &Store,
    id: &str,
    text: &str,
    range: Option<&WatchClipRange>,
) -> Result<(FrameImage, WatchContext), AppError> {
    let text = crate::ocr::recognized_text(text)?;
    let frame = image(store, id)?
        .frame
        .ok_or_else(|| AppError::new("invalid_input", "此图片不是本机视频暂停帧。"))?;
    let study = mobile_video::study(store, &frame.uri)?
        .filter(|s| s.sha256 == frame.sha256)
        .ok_or_else(|| AppError::new("conflict", "原视频资料版本已变化，请重新截图。"))?;
    let mut context = WatchContext {
        revision: frame.media_revision,
        source_id: study.source_id,
        example_id: String::new(),
        text: text.clone(),
        translation: store.sentence_translation(&text)?.unwrap_or_default(),
        translation_revision: String::new(),
        start_ms: frame.position_ms as i64,
        end_ms: frame.position_ms as i64,
        text_source: "frame_ocr".into(),
        previous: false,
        location_key: format!(
            "frame:{}:{}:{}",
            frame.sha256,
            frame.position_ms,
            crate::vocabulary::digest(text.as_bytes())
        ),
        media_sha256: frame.sha256.clone(),
        audio_stream: frame.audio_track_id.as_ref().map(|audio| mobile_video::audio_slot(store, &frame.uri, audio)).transpose()?,
        precise_timing: false,
        subtitle_start_ms: 0,
        subtitle_end_ms: 0,
    };
    context.translation_revision = crate::vocabulary::digest(context.translation.as_bytes());
    if let Some(range) = range {
        let duration: i64 = store.connection()?.query_row(
            "SELECT duration_ms FROM sources WHERE id=?",
            [&context.source_id],
            |r| r.get(0),
        )?;
        context = context.with_manual_clip(range, duration)?;
        context =
            store.bind_watch_context_verified(&frame.uri, &frame.title, &context, &frame.sha256)?;
        if let Some(audio) = &frame.audio_track_id {
            mobile_video::save(store, &mobile_video::key("actual_audio", &frame.uri), audio)?;
        }
    }
    Ok((frame, context))
}
pub(crate) fn frame_draft(
    store: &Store,
    id: &str,
    text: &str,
    range: Option<&WatchClipRange>,
) -> Result<FrameDraft, AppError> {
    let (frame, context) = frame_context(store, id, text, range)?;
    Ok(FrameDraft {
        text: context.text.clone(),
        kind: crate::ocr::infer_kind(&context.text).into(),
        context: context.text,
        meaning: context.translation,
        source_id: context.source_id.clone(),
        source_title: format!(
            "{} · 暂停帧 {} 秒{}",
            frame.title,
            frame.position_ms / 1000,
            if range.is_none() {
                "（对白区间未确定）"
            } else {
                ""
            }
        ),
        location_key: context.location_key,
        example: if range.is_some() {
            Some(store.source_example(&context.source_id, &context.example_id)?)
        } else {
            None
        },
    })
}

pub(crate) fn image_draft(store: &Store, id: &str, text: &str) -> Result<FrameDraft, AppError> {
    let image = image(store, id)?;
    let text = crate::ocr::recognized_text(text)?;
    let hash = crate::corpus::file_hash(std::path::Path::new(&image.path))?;
    let source_id = store.add_source(&crate::vocabulary::SourceInput {
        kind: "image_ocr".into(),
        title: "手机图片 OCR".into(),
        fingerprint: hash.clone(),
        path_hint: Some(image.path),
        duration_ms: None,
    })?;
    Ok(FrameDraft {
        kind: crate::ocr::infer_kind(&text).into(),
        context: text.clone(),
        meaning: String::new(),
        source_id,
        source_title: "手机图片 OCR".into(),
        location_key: format!(
            "image:{hash}:{}",
            crate::vocabulary::digest(text.as_bytes())
        ),
        text,
        example: None,
    })
}
pub(crate) fn text_draft(store: &Store, text: &str, title: &str) -> Result<FrameDraft, AppError> {
    if text.trim().is_empty() || text.chars().count() > 20000 {
        return Err(AppError::new(
            "invalid_input",
            "分享文字为空或过长，请选择较短的片段。",
        ));
    }
    let text = text.trim().to_owned();
    let hash = crate::vocabulary::digest(text.as_bytes());
    let source_id = store.add_source(&crate::vocabulary::SourceInput {
        kind: "shared_text".into(),
        title: title.into(),
        fingerprint: hash.clone(),
        path_hint: None,
        duration_ms: None,
    })?;
    Ok(FrameDraft {
        kind: crate::ocr::infer_kind(&text).into(),
        context: text.clone(),
        meaning: String::new(),
        source_id,
        source_title: title.into(),
        location_key: format!("share:{hash}"),
        text,
        example: None,
    })
}

impl Application {
    pub(crate) fn video_collection_draft(&self,revision:u64,text:String,kind:String,meaning:String)->Result<FrameDraft,AppError> {
        let learning=self.video_learning()?;
        let context=learning.context.filter(|c|c.revision==revision).ok_or_else(||AppError::new("conflict","暂停对白已变化，请重新核对。"))?;
        let state=mobile_video::state(&self.store)?;
        let title=self.store.playback_state()?.playlist.iter().find(|i|i.path==state.uri).map(|i|i.title.clone()).unwrap_or_else(||"本地视频".into());
        let bound=self.store.bind_watch_context_verified(&state.uri,&title,&context,&context.media_sha256)?;
        Ok(FrameDraft{text,kind,context:bound.text,meaning,source_id:bound.source_id.clone(),source_title:title,location_key:bound.location_key,
            example:Some(self.store.source_example(&bound.source_id,&bound.example_id)?)})
    }
    pub(crate) fn mobile_ocr_start(
        &self,
        id: String,
        rect: PixelRect,
        operation_id: String,
    ) -> Result<TaskHandle<OcrText>, AppError> {
        let image = image(&self.store, &id)?;
        let store = Arc::clone(&self.store);
        let hash = crate::vocabulary::digest(&serde_json::to_vec(&(&id, &rect))?);
        self.tasks
            .start_typed("ocr", &operation_id, &hash, move |context| {
                context.subject("手机图片 OCR");
                context.progress("ocr", 0, 1, "正在离线识别英语选区");
                context.check_cancelled()?;
                let text = recognize(&store, &image, &rect, &context.cancelled)?;
                context.check_cancelled()?;
                if let Some(frame) = &image.frame {
                    let current = mobile_video::state(&store)?;
                    if current.session_id == frame.session_id
                        && current.media_revision != frame.media_revision
                    {
                        return Err(AppError::new(
                            "conflict",
                            "视频位置或轨道已变化，请重新截图核对。",
                        ));
                    }
                }
                Ok(OcrText { image_id: id, text })
            })
    }
    pub(crate) fn mobile_frame_preview(
        &self,
        id: String,
        text: String,
        range: WatchClipRange,
        operation_id: String,
    ) -> Result<TaskHandle<crate::tasks::results::PreviewAudio>, AppError> {
        let (_, context) = frame_context(&self.store, &id, &text, Some(&range))?;
        self.preview_start(context.source_id, context.example_id, operation_id)
    }
}

pub(crate) fn release_images(store:&Store,ids:&[String])->Result<(),AppError> {
    for id in ids {
        let key=image_key(id)?;
        // Capture PNGs are disposable; vocabulary/audio/source data is never removed here.
        if mobile_video::setting::<CaptureImage>(store,&key)?.is_some() {
            let path=store.root().join("jobs").join(format!("image-{id}.png"));
            match std::fs::remove_file(path) { Ok(())=>{},Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>return Err(e.into()) }
            store.connection()?.execute("DELETE FROM settings WHERE key=?",[key])?;
        }
    }
    Ok(())
}

fn recognize(
    store: &Store,
    image: &CaptureImage,
    rect: &PixelRect,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<String, AppError> {
    #[cfg(all(target_os = "android", feature = "mobile"))]
    {
        crate::android_ocr::recognize(
            &store.root().join("ocr/tessdata"),
            std::path::Path::new(&image.path),
            rect,
            cancelled,
        )
    }
    #[cfg(not(all(target_os = "android", feature = "mobile")))]
    {
        let _ = (store, image, rect, cancelled);
        Err(AppError::new(
            "provider_unavailable",
            "此 OCR 接口用于 Android 客户端。",
        ))
    }
}
