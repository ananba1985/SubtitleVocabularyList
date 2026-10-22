//! Explicit SubDL search/download; local imports never depend on a configured account.
use crate::{
    error::AppError,
    media::MediaTools,
    store::Store,
    subtitles::{self, SubtitleBundle, SubtitleInfo, SubtitleProvenance},
    vocabulary,
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const CONFIG: &str = "subtitle:subdl";
const SECRET: &str = "subtitle-subdl";
const MAX_FILE: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(default, rename_all = "camelCase")]
pub struct SubtitleService {
    pub enabled: bool,
    pub search_on_import: bool,
    pub english_code: String,
    pub chinese_code: String,
    pub has_api_key: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(default, rename_all = "camelCase")]
pub struct SubtitleQuery {
    pub file_name: String,
    pub title: String,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub sd_id: Option<u64>,
    pub languages: Vec<String>,
    pub notice: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(default, rename_all = "camelCase")]
pub struct SubtitleTitle {
    pub id: u64,
    pub name: String,
    pub year: Option<u32>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(default, rename_all = "camelCase")]
pub struct SubtitleCandidate {
    pub id: String,
    pub title: String,
    pub title_id: u64,
    pub language: String,
    pub release_name: String,
    pub name: String,
    pub url: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub fps: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(default, rename_all = "camelCase")]
pub struct SubtitleSearch {
    pub query: SubtitleQuery,
    pub media_sha256: String,
    pub titles: Vec<SubtitleTitle>,
    pub candidates: Vec<SubtitleCandidate>,
    pub searched_at: i64,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(default, rename_all = "camelCase")]
pub struct SubtitleDownloadFile {
    pub id: String,
    pub name: String,
    pub relative_path: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(default, rename_all = "camelCase")]
pub struct SubtitleDownload {
    pub id: String,
    pub source_id: String,
    pub media_sha256: String,
    pub candidate: SubtitleCandidate,
    pub files: Vec<SubtitleDownloadFile>,
    pub downloaded_at: i64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(default, rename_all = "camelCase")]
pub struct SubtitlePair {
    pub start_ms: i64,
    pub end_ms: i64,
    pub english: String,
    pub chinese: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ipc-types", derive(ts_rs::TS))]
#[serde(default, rename_all = "camelCase")]
pub struct SubtitleDownloadPreview {
    pub file: SubtitleDownloadFile,
    pub language: String,
    pub cue_count: usize,
    pub first_ms: i64,
    pub last_ms: i64,
    pub video_duration_ms: i64,
    pub matched_sentences: usize,
    pub sentence_count: usize,
    pub pairs: Vec<SubtitlePair>,
    pub warnings: Vec<String>,
}

fn read_setting<T: DeserializeOwned + Default>(store: &Store, key: &str) -> Result<T, AppError> {
    let value: Option<String> = store
        .connection()?
        .query_row("SELECT value_json FROM settings WHERE key=?", [key], |r| {
            r.get(0)
        })
        .optional()?;
    value
        .map(|s| serde_json::from_str(&s).map_err(Into::into))
        .unwrap_or_else(|| Ok(T::default()))
}
fn write_setting<T: Serialize>(store: &Store, key: &str, value: &T) -> Result<(), AppError> {
    store.connection()?.execute("INSERT INTO settings(key,value_json) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json", params![key,serde_json::to_string(value)?])?;
    Ok(())
}
pub fn service(store: &Store) -> Result<SubtitleService, AppError> {
    let mut config: SubtitleService = read_setting(store, CONFIG)?;
    if config.english_code.is_empty() {
        config.english_code = "EN".into();
    }
    if config.chinese_code.is_empty() {
        config.chinese_code = "ZH".into();
    }
    config.has_api_key = !api_key(store)?.is_empty();
    Ok(config)
}
fn api_key(store: &Store) -> Result<String, AppError> {
    Ok(crate::providers::load_secrets(store.root(), SECRET)?
        .remove("api_key")
        .unwrap_or_default())
}
pub fn save_service(
    store: &Store,
    mut config: SubtitleService,
    key: Option<String>,
) -> Result<SubtitleService, AppError> {
    for code in [&config.english_code, &config.chinese_code] {
        if code.is_empty()
            || code.len() > 32
            || !code
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err(AppError::new(
                "invalid_input",
                "请填写服务使用的英语／中文语言代码。",
            ));
        }
    }
    let secret = match key {
        Some(value) => value,
        None => api_key(store)?,
    }
    .trim()
    .to_owned();
    if secret.len() > 1024 || secret.contains(['\r', '\n']) || config.enabled && secret.is_empty() {
        return Err(AppError::new(
            "invalid_input",
            "启用 SubDL 前请填写有效 API key。",
        ));
    }
    let mut values = HashMap::new();
    if !secret.is_empty() {
        values.insert("api_key".into(), secret);
    }
    crate::providers::save_secrets(store.root(), SECRET, &values)?;
    config.has_api_key = !values.is_empty();
    write_setting(store, CONFIG, &config)?;
    Ok(config)
}

fn ordinal(raw: &str) -> Option<u32> {
    if let Ok(n) = raw.parse() {
        return Some(n);
    }
    let digit = |c| {
        "零一二三四五六七八九"
            .chars()
            .position(|x| x == c)
            .map(|n| n as u32)
            .or_else(|| (c == '两').then_some(2))
    };
    if let Some((left, right)) = raw.split_once('十') {
        let tens = if left.is_empty() {
            1
        } else {
            digit(left.chars().next()?)?
        };
        Some(
            tens * 10
                + if right.is_empty() {
                    0
                } else {
                    digit(right.chars().next()?)?
                },
        )
    } else {
        digit(raw.chars().next()?)
    }
}
pub fn identify(file_name: &str) -> SubtitleQuery {
    let stem = Path::new(file_name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let pattern = regex::Regex::new(r"(?i)(?:s(?P<s>\d{1,3})[ ._-]*e(?P<e>\d{1,4})|(?P<x>\d{1,3})x(?P<y>\d{1,4})|第(?P<c>[零一二三四五六七八九十两\d]+)季[ ._-]*第?(?P<d>[零一二三四五六七八九十两\d]+)集)").unwrap();
    let capture = pattern.captures(&stem);
    let mut result = SubtitleQuery {
        file_name: file_name.into(),
        languages: vec!["en".into(), "zh".into()],
        ..Default::default()
    };
    let mut title_end = stem.len();
    if let Some(c) = capture {
        title_end = c.get(0).unwrap().start();
        result.season = c
            .name("s")
            .or(c.name("x"))
            .or(c.name("c"))
            .and_then(|m| ordinal(m.as_str()));
        result.episode = c
            .name("e")
            .or(c.name("y"))
            .or(c.name("d"))
            .and_then(|m| ordinal(m.as_str()));
        let rest = &stem[c.get(0).unwrap().end()..];
        if regex::Regex::new(r"(?i)^(?:[ ._-]*e\d|[ ._-]*-\d)")
            .unwrap()
            .is_match(rest)
        {
            result.episode = None;
            result.notice = "文件可能包含多集，请核对需要取得字幕的单集。".into();
        }
    }
    let year = regex::Regex::new(r"(?:^|[ ._(\[-])((?:19|20)\d{2})(?:$|[ ._)\]-])").unwrap();
    if let Some(c) = year.captures(&stem[..title_end]) {
        result.year = c.get(1).and_then(|m| m.as_str().parse().ok());
        title_end = c.get(1).unwrap().start();
    }
    result.title = stem[..title_end]
        .replace(['.', '_'], " ")
        .trim_matches([' ', '-', '[', '(', ']'])
        .to_owned();
    result
}
pub fn hint(store: &Store, source_id: &str) -> Result<SubtitleQuery, AppError> {
    let path: String = store.connection()?.query_row(
        "SELECT path_hint FROM sources WHERE id=? AND kind='video'",
        [source_id],
        |r| r.get(0),
    )?;
    let mut q = identify(
        &Path::new(&path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
    );
    if let Some(bundle) = store.subtitle_bundle(source_id)? {
        q.languages.retain(|lang| {
            !bundle
                .assets
                .iter()
                .any(|a| !a.cues.is_empty() && (&a.language == lang || a.language == "en-zh"))
        });
    }
    Ok(q)
}
pub fn cached_search(store: &Store, source_id: &str) -> Result<Option<SubtitleSearch>, AppError> {
    read_setting(store, &format!("subtitle:search:{source_id}"))
}
fn check(flag: &AtomicBool) -> Result<(), AppError> {
    if flag.load(Ordering::Relaxed) {
        Err(AppError::new("cancelled", "字幕任务已取消。"))
    } else {
        Ok(())
    }
}
fn client() -> Result<reqwest::blocking::Client, AppError> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 5
                && attempt.url().scheme() == "https"
                && attempt.previous().first().and_then(|u| u.host_str()) == attempt.url().host_str()
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .user_agent(concat!(
            "SubtitleVocabularyList/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|_| AppError::new("network_unavailable", "无法准备字幕服务连接。"))
}
fn body(
    mut response: reqwest::blocking::Response,
    limit: u64,
    flag: &AtomicBool,
) -> Result<Vec<u8>, AppError> {
    if !response.status().is_success() {
        return Err(AppError::new(
            "provider_unavailable",
            format!(
                "字幕服务返回 HTTP {}；请核对服务配置、账号额度或稍后重试。",
                response.status().as_u16()
            ),
        ));
    }
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        check(flag)?;
        let size = response
            .read(&mut chunk)
            .map_err(|_| AppError::new("network_unavailable", "字幕响应中断，请重试。"))?;
        if size == 0 {
            break;
        }
        if bytes.len() as u64 + size as u64 > limit {
            return Err(AppError::new("invalid_data", "字幕响应超过允许大小。"));
        }
        bytes.extend_from_slice(&chunk[..size]);
    }
    Ok(bytes)
}
fn number(v: &Value) -> Option<u32> {
    v.as_u64()
        .and_then(|n| n.try_into().ok())
        .or_else(|| v.as_str()?.parse().ok())
}
fn label(v: &Value) -> String {
    v.as_str().map(str::to_owned).unwrap_or_else(|| {
        if v.is_number() {
            v.to_string()
        } else {
            String::new()
        }
    })
}
fn download_url(value: &str) -> Result<String, AppError> {
    let url = reqwest::Url::parse("https://dl.subdl.com")
        .unwrap()
        .join(value)
        .map_err(|_| AppError::new("invalid_data", "字幕下载地址无效。"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("dl.subdl.com")
        || !url.path().starts_with("/subtitle/")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
    {
        return Err(AppError::new(
            "invalid_data",
            "服务返回的字幕下载地址不受支持。",
        ));
    }
    Ok(url.into())
}
pub fn search(
    store: &Store,
    source_id: &str,
    query: SubtitleQuery,
    flag: &AtomicBool,
    progress: &dyn Fn(usize, usize, &str),
) -> Result<SubtitleSearch, AppError> {
    if query.title.trim().is_empty()
        || query.title.chars().count() > 200
        || query.languages.is_empty()
        || query
            .languages
            .iter()
            .any(|s| !matches!(s.as_str(), "en" | "zh"))
        || query.languages.len() > 2
    {
        return Err(AppError::new(
            "invalid_input",
            "请核对剧名及要取得的中英语言。",
        ));
    }
    let config = service(store)?;
    if !config.enabled || !config.has_api_key {
        return Err(AppError::new(
            "not_configured",
            "请先配置并启用 SubDL 字幕服务。",
        ));
    }
    let bundle = store
        .subtitle_bundle(source_id)?
        .ok_or_else(|| AppError::new("not_found", "请先取得本地字幕资料。"))?;
    let key = api_key(store)?;
    let http = client()?;
    let mut result = SubtitleSearch {
        query: query.clone(),
        media_sha256: bundle.media_sha256,
        searched_at: chrono::Utc::now().timestamp_millis(),
        ..Default::default()
    };
    let outcome = (|| {
        for (index, lang) in query.languages.iter().enumerate() {
            check(flag)?;
            progress(index, query.languages.len(), "正在检索字幕候选");
            let mut params = vec![
                ("api_key", key.clone()),
                (
                    "languages",
                    if lang == "en" {
                        config.english_code.clone()
                    } else {
                        config.chinese_code.clone()
                    },
                ),
                ("subs_per_page", "30".into()),
                ("releases", "1".into()),
                ("unpack", "1".into()),
                ("client", "custom_integration".into()),
            ];
            if let Some(id) = query.sd_id {
                params.push(("sd_id", id.to_string()));
            } else {
                params.push(("film_name", query.title.trim().into()));
            }
            if let Some(year) = query.year {
                params.push(("year", year.to_string()));
            }
            if let Some(season) = query.season {
                params.push(("type", "tv".into()));
                params.push(("season_number", season.to_string()));
            }
            if let Some(episode) = query.episode {
                params.push(("episode_number", episode.to_string()));
            }
            let response = http
                .get("https://api.subdl.com/api/v1/subtitles")
                .query(&params)
                .send()
                .map_err(|_| AppError::new("network_unavailable", "无法连接 SubDL 字幕服务。"))?;
            let json: Value = serde_json::from_slice(&body(response, 2 * 1024 * 1024, flag)?)
                .map_err(|_| AppError::new("invalid_data", "字幕服务没有返回有效 JSON。"))?;
            if json["status"] != true {
                let message = json["error"]
                    .as_str()
                    .unwrap_or("服务未完成本次检索")
                    .replace(&key, "[已隐藏]")
                    .chars()
                    .take(200)
                    .collect::<String>();
                return Err(AppError::new(
                    "provider_unavailable",
                    format!("SubDL：{message}"),
                ));
            }
            let titles: Vec<_> = json["results"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| {
                    Some(SubtitleTitle {
                        id: number(&t["sd_id"])? as u64,
                        name: t["name"].as_str()?.into(),
                        year: number(&t["year"]),
                    })
                })
                .collect();
            for title in &titles {
                if !result.titles.iter().any(|t| t.id == title.id) {
                    result.titles.push(title.clone());
                }
            }
            // The API's subtitle list belongs to the first title. Ambiguous title searches require another explicit selection.
            if titles.len() != 1 && query.sd_id.is_none() {
                continue;
            }
            let title = titles
                .first()
                .ok_or_else(|| AppError::new("not_found", "未找到该剧名；可修改检索条件。"))?;
            if query.sd_id.is_some_and(|id| id != title.id) {
                return Err(AppError::new(
                    "invalid_data",
                    "服务返回的剧名身份与选择不符。",
                ));
            }
            for sub in json["subtitles"].as_array().into_iter().flatten() {
                let unpacked = sub["unpack_files"].as_array();
                let rows: Vec<_> = unpacked
                    .filter(|v| !v.is_empty())
                    .map(|v| v.iter().collect())
                    .unwrap_or_else(|| vec![sub]);
                for row in rows {
                    let season = number(&row["season"]).or(number(&sub["season"]));
                    let episode = number(&row["episode"]).or(number(&sub["episode"]));
                    if query.season.zip(season).is_some_and(|(a, b)| a != b)
                        || query.episode.zip(episode).is_some_and(|(a, b)| a != b)
                    {
                        continue;
                    }
                    let Some(url) = row["url"].as_str().and_then(|u| download_url(u).ok()) else {
                        continue;
                    };
                    let release_name = row["release_name"]
                        .as_str()
                        .or(sub["release_name"].as_str())
                        .unwrap_or("")
                        .to_owned();
                    let name = row["name"]
                        .as_str()
                        .or(sub["name"].as_str())
                        .unwrap_or("subtitle.zip")
                        .to_owned();
                    let candidate = SubtitleCandidate {
                        id: vocabulary::digest(format!("{}:{lang}:{url}", title.id).as_bytes()),
                        title: title.name.clone(),
                        title_id: title.id,
                        language: lang.clone(),
                        release_name,
                        name,
                        url,
                        season: season.or(query.season),
                        episode: episode.or(query.episode),
                        fps: label(&sub["fps"]),
                    };
                    if !result.candidates.iter().any(|c| c.id == candidate.id) {
                        result.candidates.push(candidate);
                    }
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = &outcome {
        result.error = Some(e.message.clone());
    }
    write_setting(store, &format!("subtitle:search:{source_id}"), &result)?;
    outcome?;
    progress(
        query.languages.len(),
        query.languages.len(),
        "字幕候选已取得，请核对剧集和发行版本",
    );
    Ok(result)
}

fn download_directory(store: &Store, id: &str) -> Result<PathBuf, AppError> {
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::new("invalid_input", "字幕下载标识无效。"));
    }
    Ok(store.root().join("cache/subtitle-download").join(id))
}
pub fn downloaded(store: &Store, id: &str) -> Result<SubtitleDownload, AppError> {
    Ok(serde_json::from_reader(std::fs::File::open(
        download_directory(store, id)?.join("manifest.json"),
    )?)?)
}
pub fn stage_download(
    store: &Store,
    source_id: &str,
    search: &SubtitleSearch,
    candidate: &SubtitleCandidate,
    bytes: &[u8],
    flag: &AtomicBool,
) -> Result<SubtitleDownload, AppError> {
    let id = vocabulary::digest(
        format!("{}:{}:{}", source_id, search.media_sha256, candidate.id).as_bytes(),
    );
    let dir = download_directory(store, &id)?;
    std::fs::create_dir_all(&dir)?;
    let mut result = SubtitleDownload {
        id,
        source_id: source_id.into(),
        media_sha256: search.media_sha256.clone(),
        candidate: candidate.clone(),
        downloaded_at: chrono::Utc::now().timestamp_millis(),
        ..Default::default()
    };
    let mut total = 0u64;
    let mut add = |name: &str, data: Vec<u8>| -> Result<(), AppError> {
        check(flag)?;
        let extension = Path::new(name)
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if !matches!(extension.as_str(), "srt" | "ass" | "ssa" | "vtt") {
            return Ok(());
        }
        total += data.len() as u64;
        if data.is_empty() || data.len() as u64 > MAX_FILE || total > 64 * 1024 * 1024 {
            return Err(AppError::new(
                "invalid_data",
                "字幕文件为空或超过允许大小。",
            ));
        }
        let hash = vocabulary::digest(&data);
        let path = dir.join(format!("{hash}.{extension}"));
        std::fs::write(&path, data)?;
        result.files.push(SubtitleDownloadFile {
            id: hash.clone(),
            name: name.into(),
            relative_path: path
                .strip_prefix(store.root())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
            sha256: hash,
        });
        Ok(())
    };
    if bytes.starts_with(b"PK") {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|_| AppError::new("invalid_data", "下载的字幕 ZIP 无效。"))?;
        if archive.len() > 128 {
            return Err(AppError::new(
                "invalid_data",
                "字幕包文件过多，请选择单集文件。",
            ));
        }
        for index in 0..archive.len() {
            check(flag)?;
            let file = archive
                .by_index(index)
                .map_err(|_| AppError::new("invalid_data", "无法读取字幕包文件。"))?;
            if file.is_dir() {
                continue;
            }
            let name = file
                .name()
                .map_err(|_| AppError::new("invalid_data", "字幕包中的文件名编码无效。"))?
                .into_owned();
            if !matches!(
                Path::new(&name)
                    .extension()
                    .and_then(|v| v.to_str())
                    .unwrap_or("")
                    .to_lowercase()
                    .as_str(),
                "srt" | "ass" | "ssa" | "vtt"
            ) {
                continue;
            }
            let mut data = Vec::new();
            file.take(MAX_FILE + 1).read_to_end(&mut data)?;
            add(&name, data)?;
        }
    } else {
        add(&candidate.name, bytes.to_vec())?;
    }
    if result.files.is_empty() {
        return Err(AppError::new(
            "invalid_data",
            "下载包中没有支持的文本字幕，请换一个候选。",
        ));
    }
    std::fs::write(dir.join("manifest.json"), serde_json::to_vec(&result)?)?;
    Ok(result)
}
pub fn download(
    store: &Store,
    source_id: &str,
    candidate_id: &str,
    flag: &AtomicBool,
) -> Result<SubtitleDownload, AppError> {
    let search = cached_search(store, source_id)?
        .ok_or_else(|| AppError::new("not_found", "请先检索字幕。"))?;
    let candidate = search
        .candidates
        .iter()
        .find(|c| c.id == candidate_id)
        .ok_or_else(|| AppError::new("not_found", "字幕候选已失效，请重新检索。"))?;
    let config = service(store)?;
    if !config.enabled || !config.has_api_key {
        return Err(AppError::new("not_configured", "请先配置字幕服务。"));
    }
    check(flag)?;
    let response = client()?
        .get(download_url(&candidate.url)?)
        .header("x-api-key", api_key(store)?)
        .send()
        .map_err(|_| AppError::new("network_unavailable", "无法下载字幕；已有本地资料保留。"))?;
    stage_download(
        store,
        source_id,
        &search,
        candidate,
        &body(response, MAX_FILE, flag)?,
        flag,
    )
}

fn parsed_download(
    store: &Store,
    tools: &MediaTools,
    download: &SubtitleDownload,
    file_id: &str,
    offset_ms: i64,
    flag: &AtomicBool,
) -> Result<(SubtitleDownloadFile, subtitles::SubtitleAsset), AppError> {
    if offset_ms.unsigned_abs() > 3_600_000 {
        return Err(AppError::new("invalid_input", "字幕偏移不能超过一小时。"));
    }
    let file = download
        .files
        .iter()
        .find(|f| f.id == file_id)
        .ok_or_else(|| AppError::new("not_found", "请先选择字幕包中的文件。"))?
        .clone();
    let path = store.root().join(&file.relative_path);
    if crate::corpus::file_hash(&path)? != file.sha256 {
        return Err(AppError::new("conflict", "下载字幕已变化，请重新下载。"));
    }
    let extension = path.extension().unwrap().to_string_lossy();
    let mut asset = subtitles::asset(
        tools,
        store.root(),
        &path,
        &download.candidate.language,
        &file.name,
        "online",
        None,
        &extension,
        flag,
    )?;
    if asset.language != download.candidate.language && asset.language != "en-zh"
        || asset.cues.is_empty()
    {
        return Err(AppError::new(
            "invalid_data",
            "文件没有所选语言的有效字幕文字，请换一个文件。",
        ));
    }
    asset.time_offset_ms = offset_ms;
    Ok((file, asset))
}
pub fn preview(
    store: &Store,
    tools: &MediaTools,
    id: &str,
    file_id: &str,
    offset_ms: i64,
    flag: &AtomicBool,
) -> Result<SubtitleDownloadPreview, AppError> {
    let download = downloaded(store, id)?;
    let (file, asset) = parsed_download(store, tools, &download, file_id, offset_ms, flag)?;
    let duration: Option<i64> = store.connection()?.query_row(
        "SELECT duration_ms FROM sources WHERE id=?",
        [&download.source_id],
        |r| r.get(0),
    )?;
    let mut result = SubtitleDownloadPreview {
        file,
        language: asset.language.clone(),
        cue_count: asset.cues.len(),
        first_ms: asset
            .cues
            .first()
            .map(|c| c.start_ms + offset_ms)
            .unwrap_or(0),
        last_ms: asset
            .cues
            .iter()
            .map(|c| c.end_ms + offset_ms)
            .max()
            .unwrap_or(0),
        video_duration_ms: duration.unwrap_or(0),
        ..Default::default()
    };
    let hint = identify(&result.file.name);
    if hint
        .season
        .zip(download.candidate.season)
        .is_some_and(|(a, b)| a != b)
        || hint
            .episode
            .zip(download.candidate.episode)
            .is_some_and(|(a, b)| a != b)
    {
        result
            .warnings
            .push("文件名的季集与候选不一致，请重新选择。".into());
    }
    if result.video_duration_ms > 0 && result.last_ms > result.video_duration_ms + 5000 {
        result
            .warnings
            .push("字幕超出视频时长，请核对发行版本或偏移。".into());
    }
    let bundle = SubtitleBundle {
        media_sha256: download.media_sha256.clone(),
        selected_chinese_id: Some(asset.id.clone()),
        assets: vec![asset.clone()],
        warnings: vec![],
    };
    let examples = {
        let db = store.connection()?;
        let mut q=db.prepare("SELECT text,start_ms,end_ms FROM examples WHERE source_id=? AND archived=0 AND start_ms IS NOT NULL AND end_ms IS NOT NULL ORDER BY start_ms")?;
        q.query_map([&download.source_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    result.sentence_count = examples.len();
    for (text, start, end) in examples {
        if let Some(chinese) = bundle.translation(start, end) {
            result.matched_sentences += 1;
            if result.pairs.len() < 5 {
                result.pairs.push(SubtitlePair {
                    start_ms: start,
                    end_ms: end,
                    english: text,
                    chinese,
                });
            }
        }
    }
    if result.pairs.is_empty() {
        result.pairs = asset
            .cues
            .iter()
            .take(5)
            .map(|c| SubtitlePair {
                start_ms: c.start_ms + offset_ms,
                end_ms: c.end_ms + offset_ms,
                english: if asset.language == "en" {
                    c.text.clone()
                } else {
                    String::new()
                },
                chinese: if asset.language != "en" {
                    c.text.clone()
                } else {
                    String::new()
                },
            })
            .collect();
    }
    Ok(result)
}
pub fn commit(
    store: &Store,
    tools: &MediaTools,
    id: &str,
    file_id: &str,
    offset_ms: i64,
    confirmed: bool,
    select_chinese: bool,
    flag: &AtomicBool,
) -> Result<SubtitleInfo, AppError> {
    if !confirmed {
        return Err(AppError::new(
            "confirmation_required",
            "请先核对剧集、发行版本及时间预览。",
        ));
    }
    let download = downloaded(store, id)?;
    let preview = preview(store, tools, id, file_id, offset_ms, flag)?;
    if !preview.warnings.is_empty() {
        return Err(AppError::new("conflict", preview.warnings.join(" ")));
    }
    let (file, mut asset) = parsed_download(store, tools, &download, file_id, offset_ms, flag)?;
    let mut bundle = store
        .subtitle_bundle(&download.source_id)?
        .ok_or_else(|| AppError::new("not_found", "来源字幕资料已失效。"))?;
    if bundle.media_sha256 != download.media_sha256 {
        return Err(AppError::new("conflict", "视频身份已变化，请重新检索。"));
    }
    let extension = Path::new(&file.relative_path)
        .extension()
        .unwrap()
        .to_string_lossy();
    let relative = format!(
        "subtitles/{}/download-{}.{}",
        bundle.media_sha256, file.sha256, extension
    );
    std::fs::copy(
        store.root().join(&file.relative_path),
        store.root().join(&relative),
    )?;
    asset.relative_path = relative;
    asset.id = vocabulary::digest(format!("{}:{}", download.candidate.id, file.sha256).as_bytes());
    asset.provenance = Some(SubtitleProvenance {
        provider: "SubDL".into(),
        source_url: download.candidate.url.clone(),
        file_id: download.candidate.id.clone(),
        release_name: download.candidate.release_name.clone(),
        title: download.candidate.title.clone(),
        season: download.candidate.season,
        episode: download.candidate.episode,
        fps: download.candidate.fps.clone(),
        downloaded_at: download.downloaded_at,
        version_confirmed: true,
    });
    let selected = select_chinese && matches!(asset.language.as_str(), "zh" | "en-zh");
    let asset_id = asset.id.clone();
    bundle.assets.retain(|a| a.id != asset.id);
    bundle.assets.push(asset);
    store.save_subtitle_bundle(&download.source_id, &bundle)?;
    store.set_subtitle_offset(&download.source_id, &asset_id, offset_ms)?;
    if selected {
        store.select_chinese_subtitle(&download.source_id, Some(asset_id))?;
    }
    let bundle = store.subtitle_bundle(&download.source_id)?.unwrap();
    store.save_subtitle_bundle(&download.source_id, &bundle)
}

pub fn maybe_search_on_import(
    store: &Store,
    source_id: &str,
    network_allowed: bool,
    flag: &AtomicBool,
    progress: &dyn Fn(usize, usize, &str),
) -> Result<(), AppError> {
    if !network_allowed {
        return Ok(());
    }
    let config = service(store)?;
    if !config.enabled || !config.search_on_import || !config.has_api_key {
        return Ok(());
    }
    let query = hint(store, source_id)?;
    if query.languages.is_empty() {
        return Ok(());
    }
    search(store, source_id, query, flag, progress).map(|_| ())
}
