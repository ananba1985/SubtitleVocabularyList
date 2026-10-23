//! Android URI adapter. Playback preferences, progress, subtitle alignment and corpus are shared.
use crate::{application::Application, error::AppError, media::{ImportedMedia, Segment}, playback::{PlaybackState, PlaylistItem}, store::Store, subtitles::{SubtitleAsset, SubtitleBundle}, tasks::TaskHandle, vocabulary};
use serde::{Deserialize, Serialize};
use rusqlite::OptionalExtension;
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct MediaRef { pub uri: String, pub title: String, pub mime: String, pub size: u64 }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct VideoTrack { pub id: String, pub format_id: String, pub kind: String, pub title: String, pub selected: bool, pub supported: bool }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct VideoState { pub session_id: String, pub uri: String, pub position_ms: u64, pub duration_ms: u64, pub playing: bool, pub paused: bool,
    pub revision: u64, pub user_paused: bool, pub foreground: bool, pub window_focused: bool, pub fullscreen: bool,
    pub media_revision: u64, pub subtitle_delay_ms:i64,
    pub ended: bool, pub speed: f64, pub tracks: Vec<VideoTrack>, pub error: Option<String> }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct VideoControl {
    pub session_id: String, pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub kind: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct VideoViewport { pub session_id: String, pub x: f64, pub y: f64, pub width: f64, pub height: f64, pub ratio: f64 }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct VideoStudy { pub source_id: String, pub sha256: String, pub subtitles: crate::subtitles::SubtitleInfo }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexedSubtitle { id: String, language: String, title: String, mime: String, cues: Vec<Segment> }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MediaIndex { sha256: String, duration_ms: i64, subtitles: Vec<IndexedSubtitle> }

pub(crate) fn invoke<T: serde::de::DeserializeOwned>(command: &str, args: serde_json::Value) -> Result<T, AppError> {
    #[cfg(all(target_os = "android", feature = "mobile"))]
    { return crate::mobile_platform::audio_invoke(command, args); }
    #[cfg(not(all(target_os = "android", feature = "mobile")))]
    { let _ = (command, args); Err(AppError::new("provider_unavailable", "此媒体接口用于 Android 客户端。")) }
}

pub(crate) fn key(name: &str, uri: &str) -> String { format!("android.{name}:{}", vocabulary::digest(uri.as_bytes())) }
pub(crate) fn setting<T: serde::de::DeserializeOwned>(store: &Store, key: &str) -> Result<Option<T>, AppError> {
    let json: Option<String> = store.connection()?.query_row("SELECT value_json FROM settings WHERE key=?", [key], |r| r.get(0)).optional()?;
    json.map(|s| serde_json::from_str(&s).map_err(Into::into)).transpose()
}
pub(crate) fn save<T: Serialize>(store: &Store, key: &str, value: &T) -> Result<(), AppError> {
    store.connection()?.execute("INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json", rusqlite::params![key,serde_json::to_string(value)?])?;
    Ok(())
}
// Keep Media3 format identities in the platform adapter; shared clip recipes use
// their stable numeric slot. Slot zero is reserved for pre-slot Android caches.
pub(crate) fn audio_slot(store: &Store, uri: &str, format_id: &str) -> Result<usize, AppError> {
    static SLOTS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = SLOTS.lock().map_err(|_| AppError::new("internal_error", "音轨信息暂不可用。"))?;
    let name = key("audio_slots", uri);
    let mut slots = setting::<std::collections::HashMap<String, usize>>(store, &name)?.unwrap_or_default();
    if let Some(slot) = slots.get(format_id) { return Ok(*slot); }
    let slot = slots.values().copied().max().unwrap_or(0) + 1;
    slots.insert(format_id.into(), slot);
    save(store, &name, &slots)?;
    Ok(slot)
}
fn reference(store: &Store, uri: &str) -> Result<MediaRef, AppError> {
    setting(store, &key("media", uri))?.ok_or_else(|| AppError::new("resource_missing", "请先通过系统选择器授权此视频。"))
}

pub(crate) fn pick(store: &Store) -> Result<PlaybackState, AppError> {
    #[derive(Deserialize)] struct Picked { files: Vec<MediaRef> }
    let mut selected: Picked = invoke("pickDocuments", serde_json::json!({"kind":"video"}))?;
    selected.files.sort_by(|a,b|crate::playback::natural_cmp(&a.title,&b.title));
    let mut playback = store.playback_state()?;
    for file in selected.files {
        save(store, &key("media", &file.uri), &file)?;
        if !playback.playlist.iter().any(|i| i.path == file.uri) {
            playback.playlist.push(PlaylistItem { path: file.uri, title: file.title, source_id: None });
        }
    }
    store.save_playback_state(&playback)?;
    Ok(playback)
}

pub(crate) fn register_shared(store:&Store,uri:&str)->Result<MediaRef,AppError> {
    let file:MediaRef = invoke("describeDocument",serde_json::json!({"uri":uri}))?;
    save(store,&key("media",&file.uri),&file)?;
    let mut playback = store.playback_state()?;
    if !playback.playlist.iter().any(|i|i.path==file.uri) { playback.playlist.push(PlaylistItem{path:file.uri.clone(),title:file.title.clone(),source_id:None}); }
    store.save_playback_state(&playback)?;
    Ok(file)
}

pub(crate) fn open(store: &Store, uri: &str) -> Result<VideoState, AppError> {
    reference(store, uri)?;
    let mut playback = store.playback_state()?;
    let progress = store.watch_progress_for(uri)?;
    let tracks = setting::<std::collections::HashMap<String,String>>(store,&key("tracks",uri))?.unwrap_or_default();
    let state: VideoState = invoke("videoOpen", serde_json::json!({"uri":uri,"sessionId":uuid::Uuid::new_v4().to_string(),
        "positionMs":progress.map(|p| (p.position * 1000.0) as u64).unwrap_or(0),"speed":playback.preferences.speed,
        "volume":if playback.preferences.muted {0.0}else{playback.preferences.volume/100.0},
        "audioKey":tracks.get("audio").cloned().unwrap_or_default(),"subtitleKey":tracks.get("subtitle").cloned().unwrap_or_default()}))?;
    playback.current_path = Some(uri.into());
    save(store,&key("verified",uri),&String::new())?;
    store.save_playback_state(&playback)?;
    if let Some(index) = setting::<MediaIndex>(store,&key("index",uri))? {
        for subtitle in index.subtitles.iter().filter(|s|s.id.starts_with("content://")||s.id.starts_with("file://")) {
            invoke::<VideoState>("videoSubtitle",serde_json::json!({"uri":uri,"subtitleUri":subtitle.id,"mime":subtitle.mime,"language":subtitle.language,"title":subtitle.title,"select":false}))?;
        }
    }
    render_delayed(store,state)
}
pub(crate) fn state(store: &Store) -> Result<VideoState, AppError> {
    static LAST_SAVE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    let state: VideoState = invoke("videoState", serde_json::json!({}))?;
    let mut last = LAST_SAVE.lock().map_err(|_|AppError::new("internal_error","播放进度暂不可用。"))?;
    if last.is_none_or(|t|t.elapsed().as_secs() >= 5) { persist_progress(store,&state)?; *last = Some(std::time::Instant::now()); }
    Ok(state)
}
pub(crate) fn control(store: &Store, control: VideoControl) -> Result<VideoState, AppError> {
    if !matches!(control.action.as_str(), "play"|"pause"|"pause_internal"|"stop"|"seek"|"speed"|"volume"|"auto_next"|"subtitle_delay"|"learning_offset"|"track"|"hide"|"fullscreen") || control.value.is_some_and(|v| !v.is_finite() || (v < 0.0 && !matches!(control.action.as_str(),"learning_offset"|"subtitle_delay"))) {
        return Err(AppError::new("invalid_input", "视频操作参数无效。"));
    }
    if control.action=="subtitle_delay" {
        let current:VideoState=invoke("videoState",serde_json::json!({}))?;
        if current.session_id!=control.session_id || control.value.unwrap_or(0.0).abs()>3600000.0 {return Err(AppError::new("invalid_input","字幕偏移或视频会话无效。"));}
        save(store,&key("subtitle_delay",&current.uri),&(control.value.unwrap_or(0.0) as i64))?;
    }
    if control.action == "learning_offset" {
        let current:VideoState=invoke("videoState",serde_json::json!({}))?;
        if current.session_id != control.session_id { return Err(AppError::new("conflict","当前视频已切换。")); }
        let study=study(store,&current.uri)?.ok_or_else(||AppError::new("resource_missing","视频资料尚未准备。"))?;
        let asset=study.subtitles.selected_chinese_id.ok_or_else(||AppError::new("resource_missing","请先添加中文字幕。"))?;
        store.set_subtitle_offset(&study.source_id,&asset,control.value.unwrap_or(0.0) as i64)?;
    }
    let state: VideoState = invoke("videoControl", serde_json::to_value(&control)?)?;
    persist_progress(store, &state)?;
    if control.action == "speed" {
        let mut playback = store.playback_state()?; playback.preferences.speed = state.speed; store.save_playback_state(&playback)?;
    }
    if matches!(control.action.as_str(),"volume"|"auto_next"|"stop") {
        let mut playback=store.playback_state()?;
        match control.action.as_str() {
            "volume"=>{playback.preferences.volume=control.value.unwrap_or(100.0).clamp(0.0,100.0);playback.preferences.muted=false;},
            "auto_next"=>playback.preferences.auto_next=control.value.unwrap_or(0.0)!=0.0,
            "stop"=>playback.current_path=None,
            _=>{}
        }
        store.save_playback_state(&playback)?;
    }
    if control.action == "track" {
        let kind = control.kind.as_deref().unwrap_or("subtitle");
        let text = control.text.as_deref().filter(|s|!s.is_empty()).unwrap_or("off");
        let mut tracks = setting::<std::collections::HashMap<String,String>>(store,&key("tracks",&state.uri))?.unwrap_or_default();
        tracks.insert(kind.into(),text.into()); save(store,&key("tracks",&state.uri),&tracks)?;
        if kind == "subtitle" && text != "off" {
            if let Some(selected) = state.tracks.iter().find(|t|t.id == text) {
                if let Some(mut index) = setting::<MediaIndex>(store,&key("index",&state.uri))? {
                    if let Some(position) = index.subtitles.iter().position(|s| (s.id == selected.format_id || selected.format_id.ends_with(&format!(":{}",s.id))) && s.cues.iter().any(|c|crate::subtitles::english_text(&c.text).is_some())) {
                        let chosen = index.subtitles.remove(position); index.subtitles.insert(0,chosen);
                        install(store,&reference(store,&state.uri)?,&index)?;
                    }
                }
            }
        }
    }
    if control.action=="subtitle_delay" || (control.action=="track" && control.kind.as_deref()==Some("subtitle")) { render_delayed(store,state) } else { Ok(state) }
}
pub(crate) fn viewport(viewport: VideoViewport) -> Result<(), AppError> {
    if [viewport.x,viewport.y,viewport.width,viewport.height,viewport.ratio].iter().any(|v| !v.is_finite()) || viewport.ratio <= 0.0 {
        return Err(AppError::new("invalid_input", "视频显示区域无效。"));
    }
    invoke::<serde_json::Value>("videoViewport", serde_json::to_value(viewport)?)?; Ok(())
}
pub(crate) fn persist_progress(store: &Store, state: &VideoState) -> Result<(), AppError> {
    if state.uri.is_empty() { return Ok(()); }
    if let Some(audio) = state.tracks.iter().find(|t|t.kind=="audio"&&t.selected) {
        audio_slot(store, &state.uri, &audio.format_id)?;
        save(store,&key("actual_audio",&state.uri),&audio.format_id)?;
    }
    store.save_watch_progress(&crate::watch::WatchProgress { path:state.uri.clone(), position:state.position_ms as f64 /1000.0, ..Default::default() })
}
pub(crate) fn study(store: &Store, uri: &str) -> Result<Option<VideoStudy>, AppError> {
    let mut study:Option<VideoStudy>=setting(store,&key("study",uri))?;
    if let Some(study)=&mut study {if let Some(bundle)=store.subtitle_bundle(&study.source_id)? {study.subtitles=bundle.info();}}
    Ok(study)
}

fn vtt_text(cues:&[Segment],delay:i64)->String {
    let stamp=|ms:i64|format!("{:02}:{:02}:{:02}.{:03}",ms/3600000,ms/60000%60,ms/1000%60,ms%1000);
    let mut vtt=String::from("WEBVTT\n\n");
    for cue in cues {let end=cue.end_ms+delay;if end<=0 {continue}let text=cue.text.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;");
        vtt.push_str(&format!("{} --> {}\n{}\n\n",stamp((cue.start_ms+delay).max(0)),stamp(end),text));}
    vtt
}

#[cfg(all(target_os="android",feature="mobile"))]
pub(crate) fn apply_prepared_episode(store:&Store,source_id:&str,document:&crate::episode_sharing::PreparedDocument)->Result<String,AppError> {
    let uri:String=store.connection()?.query_row("SELECT path_hint FROM sources WHERE id=?",[source_id],|r|r.get(0))?;
    let media=reference(store,&uri)?;
    let mut index:MediaIndex=setting(store,&key("index",&uri))?.ok_or_else(||AppError::new("resource_missing","请先完成此视频的本机索引。"))?;
    if index.sha256!=document.package.media_sha256 {return Err(AppError::new("conflict","共享资料与当前本地视频内容不匹配，请重新核对视频版本。"));}
    let packet=&document.package;
    let english=packet.cues.iter().map(|c|Segment{location_key:format!("shared:{}:{}:{}",vocabulary::digest(c.text.as_bytes()),c.start_ms,c.end_ms),text:c.text.clone(),start_ms:c.start_ms,end_ms:c.end_ms}).collect::<Vec<_>>();
    let directory=store.root().join("subtitles").join(&index.sha256);std::fs::create_dir_all(&directory)?;
    let text=vtt_text(&english,0);let path=directory.join(format!("svl-prepared-{}.vtt",vocabulary::digest(text.as_bytes())));std::fs::write(&path,text)?;
    let id=reqwest::Url::from_file_path(path).map_err(|_|AppError::new("invalid_data","本机共享字幕路径无效。"))?.to_string();
    index.subtitles.retain(|s|!s.id.starts_with("prepared:")&&!(s.id.starts_with("file://")&&s.id.contains("/svl-prepared-")));
    let mut shared=vec![IndexedSubtitle{id:id.clone(),language:"en".into(),title:format!("共享英文 v{}",document.metadata.revision),mime:"text/vtt".into(),cues:english}];
    let chinese=packet.cues.iter().filter(|c|!c.translation.trim().is_empty()).map(|c|Segment{location_key:format!("shared-cn:{}:{}:{}",vocabulary::digest(c.text.as_bytes()),c.start_ms,c.end_ms),text:c.translation.clone(),start_ms:c.start_ms,end_ms:c.end_ms}).collect::<Vec<_>>();
    if !chinese.is_empty(){shared.push(IndexedSubtitle{id:format!("prepared:{}:zh",document.metadata.digest),language:"zh".into(),title:format!("共享中文 v{}",document.metadata.revision),mime:"text/vtt".into(),cues:chinese});}
    index.subtitles.splice(0..0,shared);
    save(store,&key("subtitle_delay",&uri),&packet.subtitle_delay_ms)?;
    let result=install(store,&media,&index)?;
    if let Some(asset)=result.subtitles.assets.iter().find(|a|a.language=="zh"&&a.title.starts_with("共享中文")){store.select_chinese_subtitle(&result.source_id,Some(asset.id.clone()))?;}
    let mut tracks=setting::<std::collections::HashMap<String,String>>(store,&key("tracks",&uri))?.unwrap_or_default();tracks.insert("subtitle".into(),format!("raw:{id}"));save(store,&key("tracks",&uri),&tracks)?;
    let current=state(store)?;
    if current.uri==uri {
        invoke::<VideoState>("videoControl",serde_json::json!({"sessionId":current.session_id,"action":"pause_internal"}))?;
        let current:VideoState=invoke("videoSubtitle",serde_json::json!({"uri":uri,"subtitleUri":id,"mime":"text/vtt","language":"en","title":format!("共享英文 v{}",document.metadata.revision)}))?;
        save(store,&key("verified",&uri),&current.session_id)?;
        render_delayed_track(store,current,Some(&id))?;
    }
    Ok(result.source_id)
}

fn render_delayed(store:&Store,state:VideoState)->Result<VideoState,AppError> {
    render_delayed_track(store,state,None)
}
fn render_delayed_track(store:&Store,state:VideoState,selected_id:Option<&str>)->Result<VideoState,AppError> {
    let delay=setting::<i64>(store,&key("subtitle_delay",&state.uri))?.unwrap_or(0);
    let selected=state.tracks.iter().find(|t|t.kind=="subtitle"&&selected_id.map(|id|t.format_id==id||t.format_id.ends_with(&format!(":{id}"))).unwrap_or(t.selected));
    let mut path=String::new();
    if delay!=0 && (selected.is_some()||selected_id.is_some()) {
        let index=setting::<MediaIndex>(store,&key("index",&state.uri))?;
        if let Some(track)=index.as_ref().and_then(|i|i.subtitles.iter().find(|s|selected_id==Some(s.id.as_str()) || selected.is_some_and(|t|s.id==t.format_id || t.format_id.ends_with(&format!(":{}",s.id))))) {
            let vtt=vtt_text(&track.cues,delay);
            let file=store.root().join("jobs").join(format!("svl-shift-{}.vtt",vocabulary::digest(vtt.as_bytes())));std::fs::write(&file,vtt)?;path=file.display().to_string();
        } else {return Err(AppError::new("resource_missing","本轨没有可校正的文本索引，请添加文本字幕或继续观看。"));}
    }
    let original=selected.map(|t|t.id.clone()).or_else(||selected_id.map(|id|format!("raw:{id}"))).unwrap_or_else(||"off".into());
    invoke("videoRenderSubtitle",serde_json::json!({"sessionId":state.session_id,"renderSubtitlePath":path,"originalSubtitleKey":original,"subtitleDelayMs":delay}))
}

pub(crate) fn context(store: &Store, state: &VideoState) -> Result<Option<crate::watch::WatchContext>, AppError> {
    let Some(study) = study(store,&state.uri)? else { return Ok(None) };
    let examples = {
        let db = store.connection()?; let mut query = db.prepare("SELECT id,text,start_ms,end_ms,revision FROM examples WHERE source_id=? AND archived=0 AND start_ms IS NOT NULL AND end_ms IS NOT NULL ORDER BY start_ms,end_ms,id")?;
        query.query_map([&study.source_id], |r|Ok(crate::corpus::CandidateExample {id:r.get(0)?,text:r.get(1)?,start_ms:r.get(2)?,end_ms:r.get(3)?,revision:r.get(4)?,decision:None}))?.collect::<Result<Vec<_>,_>>()?
    };
    let media = crate::watch::WatchMedia { source_id:Some(study.source_id.clone()),path:state.uri.clone().into(),title:reference(store,&state.uri)?.title,
        subtitle_stream:None,audio_stream:state.tracks.iter().find(|t|t.kind=="audio"&&t.selected).map(|t|audio_slot(store,&state.uri,&t.format_id)).transpose()?,text_source:"android_text".into(),examples,
        subtitles:store.subtitle_bundle(&study.source_id)?,media_sha256:study.sha256 };
    let delay=setting::<i64>(store,&key("subtitle_delay",&state.uri))?.unwrap_or(0);
    let time=state.position_ms as i64-delay;
    let index=setting::<MediaIndex>(store,&key("index",&state.uri))?;
    let track=index.as_ref().and_then(|i|i.subtitles.iter().find(|s|s.cues.iter().any(|c|crate::subtitles::english_text(&c.text).is_some())));
    let cue=track.and_then(|s|s.cues.iter().find(|c|c.start_ms<=time&&time<c.end_ms).or_else(||s.cues.iter().rev().find(|c|c.end_ms<=time&&time-c.end_ms<=3000)));
    let Some(cue)=cue else { return Ok(None) };
    let start=(cue.start_ms+delay).max(0);let end=(cue.end_ms+delay).min(state.duration_ms as i64);
    if end<=start {return Ok(None)}
    let mut context=store.watch_text_context(&media,&cue.text,start,end,0,"android_text",media.audio_stream,Some((cue.start_ms,cue.end_ms)))?;
    if let Some(context)=&mut context {context.previous=time>=cue.end_ms;}
    Ok(context)
}

fn install(store: &Store, media: &MediaRef, index: &MediaIndex) -> Result<VideoStudy, AppError> {
    if index.sha256.len() != 64 || !index.sha256.bytes().all(|b| b.is_ascii_hexdigit()) { return Err(AppError::new("invalid_data", "视频摘要无效。")); }
    let mut assets = Vec::new();
    for track in &index.subtitles {
        let cues = track.cues.iter().filter(|c| c.start_ms >= 0 && c.end_ms > c.start_ms && c.text.chars().count() <= 4000).cloned().collect::<Vec<_>>();
        let sha = vocabulary::digest(&serde_json::to_vec(&cues)?);
        let id = vocabulary::digest(format!("{}:{sha}",track.id).as_bytes());
        let relative = format!("subtitles/{}/{}.json",index.sha256,id);
        let path = store.root().join(&relative); std::fs::create_dir_all(path.parent().unwrap())?; std::fs::write(path, serde_json::to_vec(&cues)?)?;
        let chinese = cues.iter().any(|c| c.text.chars().any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch)));
        let english = cues.iter().any(|c| crate::subtitles::english_text(&c.text).is_some());
        let language = if chinese && english { "en-zh" } else if chinese { "zh" } else { &track.language };
        assets.push(SubtitleAsset { id, language:language.into(), title:track.title.clone(), origin:"android_text".into(), stream_index:None,
            format:track.mime.clone(), relative_path:relative, sha256:sha, cues, time_offset_ms:0, provenance:None });
    }
    let english = assets.iter().find(|a| a.language.starts_with("en") && a.cues.iter().any(|c| crate::subtitles::english_text(&c.text).is_some()))
        .or_else(||assets.iter().find(|a| a.cues.iter().any(|c| crate::subtitles::english_text(&c.text).is_some())));
    let segments = english.map(|a| a.cues.iter().filter_map(|c| crate::subtitles::english_text(&c.text).map(|text| Segment {
        location_key:format!("android:{}:{}:{}",a.sha256,c.start_ms,c.end_ms), text,start_ms:c.start_ms,end_ms:c.end_ms,
    })).collect()).unwrap_or_default();
    let fingerprint = vocabulary::digest(format!("{}:{}", index.sha256, english.map(|a|a.sha256.as_str()).unwrap_or("no-text")).as_bytes());
    let imported = ImportedMedia { title:media.title.clone(), duration_ms:index.duration_ms, audio_path:media.uri.clone().into(), audio_stream:0, subtitle_stream:None,
        text_source:"android_text".into(), warnings:vec![], segments };
    let corpus_path = store.root().join("jobs").join(format!("android-corpus-{fingerprint}.json"));
    std::fs::write(&corpus_path,serde_json::to_vec(&imported)?)?;
    let source = store.install_corpus(std::path::Path::new(&media.uri),&fingerprint,&imported,&corpus_path)?;
    let selected_chinese_id = assets.iter().find(|a|a.language.contains("zh")).map(|a|a.id.clone());
    let warnings = if assets.iter().all(|a|a.cues.is_empty()) { vec!["尚无可靠文本字幕，可添加外置英文／中文字幕；视频可继续播放。".into()] } else { vec![] };
    let bundle = SubtitleBundle {media_sha256:index.sha256.clone(),selected_chinese_id,assets,warnings};
    let subtitles = store.save_subtitle_bundle(&source.id,&bundle)?;
    let result = VideoStudy { source_id:source.id,sha256:index.sha256.clone(),subtitles };
    save(store,&key("index", &media.uri), index)?; save(store,&key("study",&media.uri), &result)?;
    let mut playback = store.playback_state()?;
    if let Some(item) = playback.playlist.iter_mut().find(|i| i.path == media.uri) { item.source_id = Some(result.source_id.clone()); }
    store.save_playback_state(&playback)?;
    Ok(result)
}

#[cfg(all(target_os = "android", feature = "mobile"))]
pub(crate) fn export_audio(store: &Store, uri: &str, path: &std::path::Path, start_ms: i64, end_ms: i64, audio_stream: Option<usize>) -> Result<i64, AppError> {
    #[derive(Deserialize)] #[serde(rename_all = "camelCase")] struct Exported { duration_ms: i64 }
    let audio = if let Some(slot) = audio_stream.filter(|s| *s > 0) {
        let slots = setting::<std::collections::HashMap<String,usize>>(store,&key("audio_slots",uri))?.unwrap_or_default();
        Some(slots.into_iter().find(|(_,s)| *s==slot).map(|(id,_)|id)
            .ok_or_else(||AppError::new("resource_missing","收录时的音轨不可用，请重新选择视频音轨。"))?)
    } else { setting::<String>(store,&key("actual_audio",uri))? };
    let result: Exported = invoke("videoExport",serde_json::json!({"uri":uri,"path":path,"startMs":start_ms,"endMs":end_ms,"audioTrackId":audio.unwrap_or_default()}))?;
    Ok(result.duration_ms)
}

impl Application {
    pub(crate) fn video_index_start(&self, uri: String, operation_id: String) -> Result<TaskHandle<VideoStudy>, AppError> {
        let media = reference(&self.store, &uri)?; let store = Arc::clone(&self.store); let hash = vocabulary::digest(uri.as_bytes());
        let opened = state(&store)?.session_id;
        self.tasks.start_typed("video_index",&operation_id,&hash,move |context| {
            context.subject(&media.title); context.progress("subtitle_index",0,1,"正在读取本地视频指纹与文本字幕，视频可继续播放"); context.check_cancelled()?;
            let mut index: MediaIndex = invoke("mediaIndex",serde_json::json!({"uri":uri}))?; context.check_cancelled()?;
            if let Some(old) = setting::<MediaIndex>(&store,&key("index",&uri))?.filter(|old|old.sha256 == index.sha256) {
                let shared=old.subtitles.iter().filter(|s|s.id.starts_with("file://")||s.id.starts_with("prepared:")).cloned().collect::<Vec<_>>();
                for external in old.subtitles.into_iter().filter(|s|s.id.starts_with("content://")) {
                    let mut refreshed: IndexedSubtitle = invoke("subtitleRead",serde_json::json!({"uri":external.id,"title":external.title}))?;
                    refreshed.language = external.language;
                    index.subtitles.insert(0,refreshed);
                }
                index.subtitles.splice(0..0,shared);
            }
            let current=state(&store)?;
            if current.uri==uri && let Some(selected)=current.tracks.iter().find(|t|t.kind=="subtitle"&&t.selected) {
                if let Some(p)=index.subtitles.iter().position(|s|(s.id==selected.format_id||selected.format_id.ends_with(&format!(":{}",s.id))) && s.cues.iter().any(|c|crate::subtitles::english_text(&c.text).is_some())) {let track=index.subtitles.remove(p);index.subtitles.insert(0,track);}
            }
            let result = install(&store,&media,&index)?;
            let current = state(&store)?;
            if current.uri == uri && current.session_id == opened { save(&store,&key("verified",&uri),&opened)?; render_delayed(&store,current)?; }
            Ok(result)
        })
    }
    pub(crate) fn video_subtitle_add(&self, uri: String, language: String, operation_id: String) -> Result<TaskHandle<VideoStudy>, AppError> {
        let media = reference(&self.store,&uri)?;
        if !matches!(language.as_str(),"en"|"zh") { return Err(AppError::new("invalid_input","请选择英文或中文字幕。")); }
        #[derive(Deserialize)] struct Picked { files: Vec<MediaRef> }
        let picked: Picked = invoke("pickDocuments",serde_json::json!({"kind":"subtitle"}))?;
        let file = picked.files.into_iter().next().ok_or_else(||AppError::new("cancelled","已取消选择字幕。"))?;
        let store = Arc::clone(&self.store); let hash = vocabulary::digest(format!("{uri}:{}",file.uri).as_bytes());
        self.tasks.start_typed("video_subtitle",&operation_id,&hash,move |context| {
            context.subject(&file.title); context.check_cancelled()?;
            let mut index: MediaIndex = setting(&store,&key("index",&uri))?.ok_or_else(||AppError::new("resource_missing","请等待视频索引完成后添加字幕。"))?;
            let mut subtitle: IndexedSubtitle = invoke("subtitleRead",serde_json::json!({"uri":file.uri,"title":file.title}))?;
            subtitle.language = language.clone();
            invoke::<VideoState>("videoSubtitle",serde_json::json!({"uri":uri,"subtitleUri":file.uri,"mime":subtitle.mime,"language":subtitle.language,"title":file.title}))?;
            index.subtitles.retain(|s|s.id != subtitle.id); index.subtitles.insert(0,subtitle);
            context.check_cancelled()?;
            let mut result = install(&store,&media,&index)?;
            if language=="en" {
                let mut tracks=setting::<std::collections::HashMap<String,String>>(&store,&key("tracks",&uri))?.unwrap_or_default();
                tracks.insert("subtitle".into(),format!("raw:{}",file.uri));save(&store,&key("tracks",&uri),&tracks)?;
                let current=state(&store)?;if current.uri==uri {render_delayed_track(&store,current,Some(&file.uri))?;}
            }
            if language == "zh" {
                if let Some(id) = result.subtitles.assets.iter().find(|a|a.title == file.title && a.cue_count > 0 && matches!(a.language.as_str(),"zh"|"en-zh")).map(|a|a.id.clone()) {
                    result.subtitles = store.select_chinese_subtitle(&result.source_id,Some(id))?;
                    save(&store,&key("study",&uri),&result)?;
                }
            }
            Ok(result)
        })
    }
}
