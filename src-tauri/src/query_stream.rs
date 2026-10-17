use crate::{error::AppError, queries::QueryValue};
use serde_json::Value;
use std::{
    iter::Peekable,
    str::Chars,
    time::{Duration, Instant},
};

const RESPONSE_LIMIT: usize = 1_000_000;

// Decode complete SSE events, retaining partial UTF-8 and lines across packets.
#[derive(Default)]
struct Events {
    pending: Vec<u8>,
    data: Vec<String>,
    received: usize,
}
impl Events {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, AppError> {
        self.received += bytes.len();
        if self.received > RESPONSE_LIMIT {
            return Err(AppError::new("invalid_data", "服务响应过大。"));
        }
        self.pending.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.pending.drain(..=end).collect();
            let line = std::str::from_utf8(&line[..line.len() - 1])
                .map_err(|_| AppError::new("invalid_data", "模型流式响应编码无效。"))?
                .trim_end_matches('\r');
            if line.is_empty() {
                if !self.data.is_empty() {
                    events.push(std::mem::take(&mut self.data).join("\n"));
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data
                    .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
            }
        }
        Ok(events)
    }
}

pub(crate) async fn model_response(
    mut response: reqwest::Response,
    show_translation: bool,
    progress: &dyn Fn(QueryValue),
) -> Result<(String, bool), AppError> {
    if !response.status().is_success() {
        return Err(AppError::new(
            "provider_unavailable",
            format!("服务返回 HTTP {}。", response.status().as_u16()),
        ));
    }
    // Some compatible services return a complete JSON response even when asked
    // to stream. Keep accepting that response through the existing parser.
    if response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"))
    {
        let value = crate::queries::response_json_body(response).await?;
        let content = value["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| AppError::new("invalid_data", "模型没有返回有效内容。"))?;
        return Ok((
            content.to_owned(),
            value["choices"][0]["finish_reason"] == "length",
        ));
    }
    let mut events = Events::default();
    let mut content = String::new();
    let mut completed = false;
    let mut truncated = false;
    let mut last_sent: Option<(Instant, QueryValue)> = None;
    'response: while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AppError::new("network_unavailable", "模型流式响应中断，请重新查询。"))?
    {
        for event in events.push(&chunk)? {
            if event.trim() == "[DONE]" {
                completed = true;
                break 'response;
            }
            let value: Value = serde_json::from_str(&event)
                .map_err(|_| AppError::new("invalid_data", "模型流式事件不是有效 JSON。"))?;
            if let Some(error) = value.get("error") {
                return Err(AppError::new(
                    "provider_unavailable",
                    error["message"].as_str().unwrap_or("模型返回流式错误。"),
                ));
            }
            let choice = &value["choices"][0];
            if let Some(reason) = choice["finish_reason"].as_str() {
                completed = true;
                truncated = reason == "length";
            }
            if let Some(delta) = choice["delta"]["content"].as_str() {
                content.push_str(delta);
                let mut preview = preview_value(&content);
                if !show_translation {
                    preview.translation.clear();
                }
                let has_text = !preview.meaning.is_empty()
                    || !preview.translation.is_empty()
                    || !preview.notes.is_empty();
                if has_text
                    && last_sent.as_ref().is_none_or(|(time, old)| {
                        time.elapsed() >= Duration::from_millis(80) && old != &preview
                    })
                {
                    progress(preview.clone());
                    last_sent = Some((Instant::now(), preview));
                }
            }
        }
    }
    if !completed {
        return Err(AppError::new(
            "network_unavailable",
            "模型流式响应未完整结束，请重新查询。",
        ));
    }
    if content.trim().is_empty() {
        return Err(AppError::new("invalid_data", "模型流式响应没有返回正文。"));
    }
    Ok((content, truncated))
}

// Only expose decoded field text. JSON syntax and an unfinished escape sequence
// stay internal; final parsing and validation still use the complete response.
fn preview_value(content: &str) -> QueryValue {
    let content = content
        .trim_start()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_start();
    let mut chars = content.chars().peekable();
    let mut value = QueryValue {
        meaning: String::new(),
        translation: String::new(),
        notes: String::new(),
        source_url: String::new(),
    };
    if chars.next() != Some('{') {
        return value;
    }
    loop {
        whitespace(&mut chars);
        let Some((key, true)) = string(&mut chars) else {
            break;
        };
        whitespace(&mut chars);
        if chars.next() != Some(':') {
            break;
        }
        whitespace(&mut chars);
        let Some((text, closed)) = string(&mut chars) else {
            break;
        };
        match key.as_str() {
            "meaning" => value.meaning = text,
            "translation" => value.translation = text,
            "notes" => value.notes = text,
            _ => {}
        }
        if !closed {
            break;
        }
        whitespace(&mut chars);
        if chars.next() != Some(',') {
            break;
        }
    }
    value
}
fn whitespace(chars: &mut Peekable<Chars<'_>>) {
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
}
fn hex(chars: &mut Peekable<Chars<'_>>) -> Option<u32> {
    (0..4).try_fold(0, |value, _| Some(value * 16 + chars.next()?.to_digit(16)?))
}
fn string(chars: &mut Peekable<Chars<'_>>) -> Option<(String, bool)> {
    if chars.next()? != '"' {
        return None;
    }
    let mut text = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some((text, true)),
            '\\' => {
                let Some(escape) = chars.next() else {
                    break;
                };
                let decoded = match escape {
                    '"' | '\\' | '/' => escape,
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'u' => {
                        let Some(mut code) = hex(chars) else {
                            break;
                        };
                        if (0xd800..=0xdbff).contains(&code) {
                            if chars.next() != Some('\\') || chars.next() != Some('u') {
                                break;
                            }
                            let Some(low) =
                                hex(chars).filter(|low| (0xdc00..=0xdfff).contains(low))
                            else {
                                break;
                            };
                            code = 0x10000 + ((code - 0xd800) << 10) + low - 0xdc00;
                        }
                        let Some(c) = char::from_u32(code) else {
                            break;
                        };
                        c
                    }
                    _ => break,
                };
                text.push(decoded);
            }
            c => text.push(c),
        }
    }
    Some((text, false))
}
