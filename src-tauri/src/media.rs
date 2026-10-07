use crate::{error::AppError, vocabulary::digest};
use image::{GrayImage, Luma};
use libbitsub_core::PgsParser;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaTools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub tesseract: PathBuf,
    pub whisper: PathBuf,
    pub whisper_model: PathBuf,
}

impl MediaTools {
    pub fn development(runtime: impl AsRef<Path>) -> Self {
        let runtime = runtime.as_ref();
        Self {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            tesseract: "tesseract".into(),
            whisper: runtime.join("whisper/Release/whisper-cli.exe"),
            whisper_model: runtime.join("ggml-base.en.bin"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stream {
    pub index: usize,
    pub codec_type: String,
    #[serde(default)]
    pub codec_name: String,
    #[serde(default)]
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    pub streams: Vec<Stream>,
    pub format: ProbeFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeFormat {
    pub duration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub location_key: String,
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedMedia {
    pub title: String,
    pub duration_ms: i64,
    pub audio_path: PathBuf,
    pub audio_stream: usize,
    pub subtitle_stream: Option<usize>,
    pub text_source: String,
    pub warnings: Vec<String>,
    pub segments: Vec<Segment>,
}

pub fn run_tool(
    program: &Path,
    arguments: &[String],
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, AppError> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(AppError::new("cancelled", "任务已取消。"));
    }
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("OMP_THREAD_LIMIT", "2");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|e| {
        AppError::new(
            "provider_unavailable",
            format!("无法运行 {}：{e}", program.display()),
        )
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::new("internal_error", "未取得工具输出流。"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::new("internal_error", "未取得工具诊断流。"))?;
    let output_thread = thread::spawn(move || {
        let mut output = Vec::new();
        stdout
            .take(64 * 1024 * 1024)
            .read_to_end(&mut output)
            .map(|_| output)
    });
    let error_thread = thread::spawn(move || {
        let mut output = Vec::new();
        stderr
            .take(8 * 1024 * 1024)
            .read_to_end(&mut output)
            .map(|_| output)
    });
    let mut was_cancelled = false;
    let status = loop {
        if cancelled.load(Ordering::Relaxed) {
            was_cancelled = true;
            let _ = child.kill();
            break child.wait()?;
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        thread::sleep(Duration::from_millis(40));
    };
    let output = output_thread
        .join()
        .map_err(|_| AppError::new("internal_error", "工具输出读取中断。"))??;
    let errors = error_thread
        .join()
        .map_err(|_| AppError::new("internal_error", "工具诊断读取中断。"))??;
    if was_cancelled {
        return Err(AppError::new("cancelled", "任务已取消，已完成内容保留。"));
    }
    if !status.success() {
        let detail = String::from_utf8_lossy(&errors);
        let tail: String = detail
            .chars()
            .rev()
            .take(2500)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        return Err(AppError::new(
            "provider_unavailable",
            format!("{} 执行失败：{tail}", program.display()),
        ));
    }
    Ok(output)
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

pub fn probe(tools: &MediaTools, input: &Path, cancelled: &AtomicBool) -> Result<Probe, AppError> {
    let result = run_tool(
        &tools.ffprobe,
        &strings(&[
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
            &input.to_string_lossy(),
        ]),
        cancelled,
    )?;
    Ok(serde_json::from_slice(&result)?)
}

fn english(stream: &Stream) -> bool {
    matches!(
        stream.tags.get("language").map(String::as_str),
        Some("en" | "eng" | "en-US" | "en-GB")
    )
}

pub fn import_media(
    tools: &MediaTools,
    input: &Path,
    directory: &Path,
    force_speech: bool,
    cancelled: &AtomicBool,
    progress: &dyn Fn(&str, usize, usize),
) -> Result<ImportedMedia, AppError> {
    std::fs::create_dir_all(directory)?;
    progress("probe", 0, 1);
    let probe = probe(tools, input, cancelled)?;
    let duration = probe
        .format
        .duration
        .parse::<f64>()
        .map_err(|_| AppError::new("invalid_data", "素材时长无法读取。"))?;
    if !duration.is_finite() || duration <= 0.0 {
        return Err(AppError::new("invalid_data", "素材没有有效时长。"));
    }
    let duration_ms = (duration * 1000.0).round() as i64;
    let audio_stream = probe
        .streams
        .iter()
        .find(|s| s.codec_type == "audio" && english(s))
        .or_else(|| probe.streams.iter().find(|s| s.codec_type == "audio"))
        .ok_or_else(|| AppError::new("unsupported", "素材没有可用音轨。"))?;
    let subtitles: Vec<_> = probe
        .streams
        .iter()
        .filter(|s| s.codec_type == "subtitle" && english(s))
        .collect();
    let text_subtitle = subtitles.iter().find(|s| {
        matches!(
            s.codec_name.as_str(),
            "subrip" | "ass" | "ssa" | "webvtt" | "mov_text" | "text"
        )
    });
    let pgs_subtitle = subtitles
        .iter()
        .find(|s| s.codec_name == "hdmv_pgs_subtitle");
    let audio = directory.join("audio.m4a");
    progress("audio", 0, 1);
    run_tool(
        &tools.ffmpeg,
        &strings(&[
            "-nostdin",
            "-y",
            "-v",
            "error",
            "-i",
            &input.to_string_lossy(),
            "-map",
            &format!("0:{}", audio_stream.index),
            "-vn",
            "-af",
            "aresample=async=1:first_pts=0",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            &audio.to_string_lossy(),
        ]),
        cancelled,
    )?;
    progress("audio", 1, 1);
    let mut warnings = Vec::new();
    let mut subtitle_index = None;
    let (segments, text_source) = if !force_speech && let Some(stream) = text_subtitle {
        subtitle_index = Some(stream.index);
        let path = directory.join("original-subtitles.srt");
        run_tool(
            &tools.ffmpeg,
            &strings(&[
                "-nostdin",
                "-y",
                "-v",
                "error",
                "-i",
                &input.to_string_lossy(),
                "-map",
                &format!("0:{}", stream.index),
                "-f",
                "srt",
                &path.to_string_lossy(),
            ]),
            cancelled,
        )?;
        (
            parse_srt(&std::fs::read_to_string(path)?)?,
            "embedded_text".to_owned(),
        )
    } else if !force_speech && let Some(stream) = pgs_subtitle {
        subtitle_index = Some(stream.index);
        let path = directory.join("original-subtitles.sup");
        run_tool(
            &tools.ffmpeg,
            &strings(&[
                "-nostdin",
                "-y",
                "-v",
                "error",
                "-i",
                &input.to_string_lossy(),
                "-map",
                &format!("0:{}", stream.index),
                "-c",
                "copy",
                &path.to_string_lossy(),
            ]),
            cancelled,
        )?;
        match ocr_pgs(tools, &path, directory, duration_ms, cancelled, progress) {
            Ok(segments) => (segments, "pgs_ocr".to_owned()),
            Err(error) if error.code != "cancelled" => {
                warnings.push(format!(
                    "图片字幕识别未完成，改用本地语音转写：{}",
                    error.message
                ));
                (
                    transcribe(tools, &audio, directory, cancelled, progress)?,
                    "local_speech".to_owned(),
                )
            }
            Err(error) => return Err(error),
        }
    } else {
        (
            transcribe(tools, &audio, directory, cancelled, progress)?,
            "local_speech".to_owned(),
        )
    };
    if segments.is_empty() {
        return Err(AppError::new(
            "invalid_data",
            "未取得有效对白文字，请检查素材或识别资源。",
        ));
    }
    for segment in &segments {
        if segment.start_ms < 0
            || segment.end_ms <= segment.start_ms
            || segment.end_ms > duration_ms + 500
        {
            return Err(AppError::new(
                "invalid_data",
                "字幕或识别时间超出素材范围，请修正后继续。",
            ));
        }
    }
    let result = ImportedMedia {
        title: input
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        duration_ms,
        audio_path: audio,
        audio_stream: audio_stream.index,
        subtitle_stream: subtitle_index,
        text_source,
        warnings,
        segments,
    };
    std::fs::write(
        directory.join("corpus.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    progress("ready", 1, 1);
    Ok(result)
}

fn timestamp(text: &str) -> Result<i64, AppError> {
    let parts: Vec<_> = text.trim().split([':', ',', '.']).collect();
    if parts.len() != 4 {
        return Err(AppError::new("invalid_data", "字幕时间格式无效。"));
    }
    let values = parts
        .iter()
        .map(|part| part.parse::<i64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::new("invalid_data", "字幕时间包含无效数值。"))?;
    if values[0] < 0
        || !(0..60).contains(&values[1])
        || !(0..60).contains(&values[2])
        || !(0..1000).contains(&values[3])
    {
        return Err(AppError::new("invalid_data", "字幕时间范围无效。"));
    }
    values[0]
        .checked_mul(60)
        .and_then(|value| value.checked_add(values[1]))
        .and_then(|value| value.checked_mul(60))
        .and_then(|value| value.checked_add(values[2]))
        .and_then(|value| value.checked_mul(1000))
        .and_then(|value| value.checked_add(values[3]))
        .ok_or_else(|| AppError::new("invalid_data", "字幕时间数值过大。"))
}

pub fn parse_srt(text: &str) -> Result<Vec<Segment>, AppError> {
    let normalized = text
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let tags = regex::Regex::new(r"</?(?:i|b|u|font|c)(?:\s[^>]*)?>")
        .expect("fixed subtitle markup pattern");
    let mut result = Vec::new();
    for block in normalized
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
    {
        let lines: Vec<_> = block.lines().collect();
        let position = lines.iter().position(|line| line.contains("-->"));
        let Some(position) = position else {
            return Err(AppError::new("invalid_data", "字幕片段缺少时间位置。"));
        };
        let times: Vec<_> = lines[position].split("-->").collect();
        let start = timestamp(times[0])?;
        let end = timestamp(times[1].split_whitespace().next().unwrap_or(""))?;
        if end <= start {
            return Err(AppError::new("invalid_data", "字幕结束时间早于开始时间。"));
        }
        let raw = lines[position + 1..].join("\n");
        let content = tags.replace_all(&raw, "").trim().to_owned();
        if !content.is_empty() {
            result.push(Segment {
                location_key: format!("cue:{}", result.len()),
                text: content,
                start_ms: start,
                end_ms: end,
            });
        }
    }
    Ok(result)
}

pub fn transcribe(
    tools: &MediaTools,
    audio: &Path,
    directory: &Path,
    cancelled: &AtomicBool,
    progress: &dyn Fn(&str, usize, usize),
) -> Result<Vec<Segment>, AppError> {
    if !tools.whisper.is_file() || !tools.whisper_model.is_file() {
        return Err(AppError::new(
            "resource_missing",
            "本地语音识别程序或模型尚未准备完成。",
        ));
    }
    let wav = directory.join("speech-16k.wav");
    run_tool(
        &tools.ffmpeg,
        &strings(&[
            "-nostdin",
            "-y",
            "-v",
            "error",
            "-i",
            &audio.to_string_lossy(),
            "-ar",
            "16000",
            "-ac",
            "1",
            &wav.to_string_lossy(),
        ]),
        cancelled,
    )?;
    progress("transcribe", 0, 1);
    let base = directory.join("transcribed");
    run_tool(
        &tools.whisper,
        &strings(&[
            "-m",
            &tools.whisper_model.to_string_lossy(),
            "-f",
            &wav.to_string_lossy(),
            "-l",
            "en",
            "-t",
            "2",
            "-ojf",
            "-of",
            &base.to_string_lossy(),
        ]),
        cancelled,
    )?;
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(base.with_extension("json"))?)?;
    let entries = json["transcription"]
        .as_array()
        .ok_or_else(|| AppError::new("invalid_data", "转写结果缺少对白列表。"))?;
    let mut result = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let content = entry["text"].as_str().unwrap_or("").trim();
        let start = entry["offsets"]["from"]
            .as_i64()
            .ok_or_else(|| AppError::new("invalid_data", "转写结果缺少开始时间。"))?;
        let end = entry["offsets"]["to"]
            .as_i64()
            .ok_or_else(|| AppError::new("invalid_data", "转写结果缺少结束时间。"))?;
        if !content.is_empty() && end > start {
            result.push(Segment {
                location_key: format!("speech:{index}"),
                text: content.to_owned(),
                start_ms: start,
                end_ms: end,
            });
        }
    }
    progress("transcribe", 1, 1);
    Ok(result)
}

pub fn ocr_pgs(
    tools: &MediaTools,
    sup: &Path,
    directory: &Path,
    duration_ms: i64,
    cancelled: &AtomicBool,
    progress: &dyn Fn(&str, usize, usize),
) -> Result<Vec<Segment>, AppError> {
    let bytes = std::fs::read(sup)?;
    let mut parser = PgsParser::new();
    let count = parser.parse(&bytes);
    if count == 0 {
        return Err(AppError::new(
            "invalid_data",
            "PGS 字幕没有有效的显示片段。",
        ));
    }
    let mut results: Vec<Segment> = Vec::new();
    let mut recognized = HashMap::<String, String>::new();
    let mut empty_visible_cues = 0;
    for index in 0..count {
        if cancelled.load(Ordering::Relaxed) {
            return Err(AppError::new("cancelled", "图片字幕识别已取消。"));
        }
        if parser.get_cue_composition_count(index) == 0 {
            continue;
        }
        let start = parser.get_cue_start_time(index).round() as i64;
        let end = (parser.get_cue_end_time(index).round() as i64).min(duration_ms);
        if end <= start {
            continue;
        }
        let frame = parser.render_at_index(index).ok_or_else(|| {
            AppError::new(
                "invalid_data",
                format!("PGS 片段 {index} 解码失败：{}", parser.last_render_issue()),
            )
        })?;
        if frame.compositions.is_empty() {
            return Err(AppError::new("invalid_data", "PGS 可见片段没有图像。"));
        }
        let min_x = frame
            .compositions
            .iter()
            .map(|c| c.x as u32)
            .min()
            .unwrap_or(0);
        let min_y = frame
            .compositions
            .iter()
            .map(|c| c.y as u32)
            .min()
            .unwrap_or(0);
        let max_x = frame
            .compositions
            .iter()
            .map(|c| c.x as u32 + c.width as u32)
            .max()
            .unwrap_or(0);
        let max_y = frame
            .compositions
            .iter()
            .map(|c| c.y as u32 + c.height as u32)
            .max()
            .unwrap_or(0);
        if max_x <= min_x
            || max_y <= min_y
            || (max_x - min_x) as u64 * (max_y - min_y) as u64 > 16_777_216
        {
            return Err(AppError::new("invalid_data", "PGS 图像尺寸无效。"));
        }
        let mut image = GrayImage::from_pixel(max_x - min_x + 40, max_y - min_y + 40, Luma([255]));
        for composition in frame.compositions {
            if composition.rgba.len()
                != composition.width as usize * composition.height as usize * 4
            {
                return Err(AppError::new("invalid_data", "PGS 图像数据长度无效。"));
            }
            for y in 0..composition.height as u32 {
                for x in 0..composition.width as u32 {
                    let pos = ((y * composition.width as u32 + x) * 4) as usize;
                    let pixel = &composition.rgba[pos..pos + 4];
                    if pixel[3] < 24 {
                        continue;
                    }
                    let luminance =
                        ((pixel[0] as u32 * 299 + pixel[1] as u32 * 587 + pixel[2] as u32 * 114)
                            / 1000) as u8;
                    if luminance > 128 {
                        image.put_pixel(
                            composition.x as u32 - min_x + x + 20,
                            composition.y as u32 - min_y + y + 20,
                            Luma([255 - luminance]),
                        );
                    }
                }
            }
        }
        let key = format!(
            "{}x{}:{}",
            image.width(),
            image.height(),
            digest(image.as_raw())
        );
        let text = if let Some(text) = recognized.get(&key) {
            text.clone()
        } else {
            let path = directory.join(format!("pgs-{index}.png"));
            image
                .save(&path)
                .map_err(|e| AppError::new("io_error", e.to_string()))?;
            let output = run_tool(
                &tools.tesseract,
                &strings(&[
                    &path.to_string_lossy(),
                    "stdout",
                    "-l",
                    "eng",
                    "--psm",
                    "6",
                    "--dpi",
                    "300",
                ]),
                cancelled,
            )?;
            let text = String::from_utf8_lossy(&output).trim().to_owned();
            if index > 4 {
                let _ = std::fs::remove_file(&path);
            }
            recognized.insert(key, text.clone());
            text
        };
        if text.is_empty() {
            empty_visible_cues += 1;
            continue;
        }
        if let Some(previous) = results.last_mut()
            && previous.text == text
            && start <= previous.end_ms + 50
        {
            previous.end_ms = end;
            continue;
        }
        results.push(Segment {
            location_key: format!("pgs:{index}"),
            text,
            start_ms: start,
            end_ms: end,
        });
        progress("pgs_ocr", index + 1, count);
    }
    if results.is_empty() || empty_visible_cues > 0 {
        return Err(AppError::new(
            "invalid_data",
            format!("图片字幕有 {empty_visible_cues} 个可见片段未能识别。"),
        ));
    }
    progress("pgs_ocr", count, count);
    Ok(results)
}

pub fn extract_clip(
    tools: &MediaTools,
    audio: &Path,
    output: &Path,
    start_ms: i64,
    end_ms: i64,
    cancelled: &AtomicBool,
) -> Result<i64, AppError> {
    if start_ms < 0 || end_ms <= start_ms {
        return Err(AppError::new("invalid_input", "对白截取范围无效。"));
    }
    let parent = output
        .parent()
        .ok_or_else(|| AppError::new("invalid_input", "音频目标位置无效。"))?;
    std::fs::create_dir_all(parent)?;
    run_tool(
        &tools.ffmpeg,
        &strings(&[
            "-nostdin",
            "-y",
            "-v",
            "error",
            "-ss",
            &format!("{:.3}", start_ms as f64 / 1000.0),
            "-i",
            &audio.to_string_lossy(),
            "-t",
            &format!("{:.3}", (end_ms - start_ms) as f64 / 1000.0),
            "-vn",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            &output.to_string_lossy(),
        ]),
        cancelled,
    )?;
    let inspected = probe(tools, output, cancelled)?;
    let duration = inspected
        .format
        .duration
        .parse::<f64>()
        .map_err(|_| AppError::new("invalid_data", "截取的音频时长无效。"))?;
    if !duration.is_finite()
        || duration <= 0.0
        || inspected.streams.iter().all(|s| s.codec_type != "audio")
    {
        return Err(AppError::new("invalid_data", "截取结果没有有效音频。"));
    }
    Ok((duration * 1000.0).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt_keeps_original_times_and_overlapping_dialogue() {
        let data = "1\r\n00:00:01,000 --> 00:00:03,000\r\n<i>Hello.</i>\r\n\r\n2\r\n00:00:02,900 --> 00:00:04,000\r\nYes.\r\n";
        let segments = parse_srt(data).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Hello.");
        assert_eq!(segments[1].start_ms, 2900);
    }
    #[test]
    fn malformed_times_are_not_successful_subtitles() {
        assert!(parse_srt("1\n00:00:03,000 --> 00:00:01,000\nHello.").is_err());
        assert!(parse_srt("1\n00:70:03,000 --> 00:70:05,000\nHello.").is_err());
        assert!(parse_srt("1\nHello.").is_err());
        assert!(
            parse_srt("1\n9223372036854775807:00:00,000 --> 9223372036854775807:00:01,000\nHello.")
                .is_err()
        );
    }
}
