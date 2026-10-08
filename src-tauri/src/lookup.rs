use crate::{application::Application, error::AppError, tasks::TaskSnapshot, vocabulary::digest};
use scraper::{ElementRef, Html, Selector};
use serde::Serialize;
use serde_json::Value;
use std::{io::Read, time::Duration};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Definition {
    text: String,
    part_of_speech: String,
    example: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupResult {
    query: String,
    source: String,
    source_url: String,
    definitions: Vec<Definition>,
}
fn missing() -> AppError {
    AppError::new(
        "not_found",
        "没有查到可用英语释义，请使用本地解释或在线中文翻译。",
    )
}
fn dictionary(query: &str, value: &Value) -> Result<LookupResult, AppError> {
    let html = value["parse"]["text"].as_str().ok_or_else(missing)?;
    let document = Html::parse_document(html);
    let mut english = false;
    let mut part = String::new();
    let mut definitions = vec![];
    let example_selector = Selector::parse("dl").unwrap();
    for node in document.root_element().descendants() {
        let Some(element) = ElementRef::wrap(node) else {
            continue;
        };
        let name = element.value().name();
        if name == "h2" {
            english = element.value().id() == Some("English")
                || element.text().collect::<String>().trim() == "English";
            part.clear();
            continue;
        }
        if !english {
            continue;
        }
        if matches!(name, "h3" | "h4") {
            let heading = element.text().collect::<String>();
            let heading = heading.trim();
            part = if matches!(
                heading,
                "Noun"
                    | "Verb"
                    | "Adjective"
                    | "Adverb"
                    | "Phrase"
                    | "Preposition"
                    | "Conjunction"
                    | "Pronoun"
                    | "Interjection"
                    | "Proverb"
                    | "Determiner"
                    | "Numeral"
                    | "Proper noun"
                    | "Particle"
            ) {
                heading.into()
            } else {
                String::new()
            };
        }
        if name != "li"
            || part.is_empty()
            || element
                .parent()
                .and_then(ElementRef::wrap)
                .is_none_or(|p| p.value().name() != "ol")
        {
            continue;
        }
        let mut text = String::new();
        for child in element.descendants() {
            if let Some(fragment) = child.value().as_text() {
                let excluded = child
                    .ancestors()
                    .take_while(|a| a.id() != element.id())
                    .filter_map(ElementRef::wrap)
                    .any(|e| matches!(e.value().name(), "dl" | "ul" | "ol" | "sup"));
                if !excluded {
                    text.push_str(fragment);
                }
            }
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !text.is_empty() && text.chars().count() <= 4000 {
            let example = element
                .select(&example_selector)
                .next()
                .map(|e| {
                    e.text()
                        .collect::<String>()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            definitions.push(Definition {
                text,
                part_of_speech: part.clone(),
                example,
            });
        }
    }
    if definitions.is_empty() {
        return Err(missing());
    }
    let mut source = reqwest::Url::parse("https://en.wiktionary.org/wiki/").unwrap();
    source
        .path_segments_mut()
        .unwrap()
        .pop_if_empty()
        .push(query);
    Ok(LookupResult {
        query: query.into(),
        source: "Wiktionary · 英语词典".into(),
        source_url: source.to_string(),
        definitions,
    })
}
fn translation(query: &str, value: &Value) -> Result<LookupResult, AppError> {
    if value["responseStatus"].as_i64() != Some(200) {
        return Err(AppError::new(
            "network_error",
            "在线翻译未完成或免费服务额度不足，当前草稿保留。",
        ));
    }
    let text = value["responseData"]["translatedText"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| AppError::new("invalid_data", "在线翻译没有返回有效文字，当前草稿保留。"))?;
    Ok(LookupResult {
        query: query.into(),
        source: "MyMemory · 中文机器翻译，请核对语境".into(),
        source_url: "https://mymemory.translated.net/".into(),
        definitions: vec![Definition {
            text: text.into(),
            part_of_speech: String::new(),
            example: String::new(),
        }],
    })
}
impl Application {
    pub fn online_query_start(
        &self,
        text: String,
        provider: String,
        operation_id: String,
    ) -> Result<TaskSnapshot, AppError> {
        if self.settings()?.offline_mode {
            return Err(AppError::new(
                "offline_mode",
                "当前为离线模式，请在本地设置允许按需联网后再查询。",
            ));
        }
        let text = text.trim().to_owned();
        if !matches!(provider.as_str(), "dictionary" | "translation")
            || text.is_empty()
            || text.contains('\0')
            || text.len() > 500
            || (provider == "dictionary" && text.chars().count() > 120)
        {
            return Err(AppError::new(
                "invalid_input",
                "词典查询最多 120 个字符；在线翻译最多 500 UTF-8 字节，请缩短查询文字。",
            ));
        }
        let hash = digest(serde_json::to_vec(&(&text, &provider))?.as_slice());
        self.tasks.start("online_query",&operation_id,&hash,move|context|{
            context.subject(&text);
            context.check_cancelled()?;context.progress("query",0,1,"正在执行本次在线查询，仅发送当前查询文字");
            let mut url=reqwest::Url::parse(if provider=="translation"{"https://api.mymemory.translated.net/get"}else{"https://en.wiktionary.org/w/api.php"}).unwrap();
            if provider=="translation"{url.query_pairs_mut().extend_pairs([("q",text.as_str()),("langpair","en|zh-CN")]);}
            else{url.query_pairs_mut().extend_pairs([("action","parse"),("page",text.as_str()),("prop","text"),("format","json"),("formatversion","2"),("disableeditsection","1")]);}
            let client=reqwest::blocking::Client::builder().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(20)).user_agent("SubtitleVocabularyList/0.1 (https://github.com/ananba1985/SubtitleVocabularyList)").build().map_err(|_|AppError::new("network_error","无法准备在线查询，当前草稿保留。"))?;
            let response=client.get(url).send().map_err(|_|AppError::new("network_error","在线查询暂不可用，当前草稿保留，可稍后重试。"))?;
            context.check_cancelled()?;
            if !response.status().is_success(){return Err(AppError::new("network_error","在线查询请求未完成，当前草稿保留。"));}
            let mut bytes=vec![];response.take(1_000_001).read_to_end(&mut bytes).map_err(|_|AppError::new("network_error","在线查询响应中断，当前草稿保留。"))?;context.check_cancelled()?;
            if bytes.len()>1_000_000{return Err(AppError::new("invalid_data","在线查询响应过大，当前草稿保留。"));}
            let value:Value=serde_json::from_slice(&bytes).map_err(|_|AppError::new("invalid_data","在线查询响应无法读取，当前草稿保留。"))?;
            Ok(serde_json::to_value(if provider=="translation"{translation(&text,&value)?}else{dictionary(&text,&value)?})?)
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{application::Settings, store::Store, tasks::TaskManager};
    use std::sync::Arc;
    #[test]
    fn offline_requests_and_multilingual_results_never_write_vocabulary() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(dir.path()).unwrap());
        let tasks = TaskManager::new(Arc::clone(&store), Arc::new(|_| {}));
        let app = Application::new(Arc::clone(&store), Arc::clone(&tasks), Settings::default());
        assert_eq!(
            app.online_query_start(
                "reluctant".into(),
                "dictionary".into(),
                uuid::Uuid::new_v4().to_string()
            )
            .unwrap_err()
            .code,
            "offline_mode"
        );
        assert!(tasks.list().unwrap().is_empty());
        let result=dictionary("reluctant",&serde_json::json!({"parse":{"text":"<h2 id='French'>French</h2><h3>Adjective</h3><ol><li>Wrong language.</li></ol><h2 id='English'>English</h2><h3>Adjective</h3><ol><li>Not willing to act.<dl><dd>A synthetic example.</dd></dl></li></ol><h2 id='Dutch'>Dutch</h2><h3>Noun</h3><ol><li>Another language.</li></ol>"}})).unwrap();
        assert_eq!(result.definitions.len(), 1);
        assert_eq!(result.definitions[0].text, "Not willing to act.");
        assert_eq!(result.definitions[0].example, "A synthetic example.");
        assert!(store.list_entries("", 0, 100).unwrap().is_empty());
        assert!(translation("x", &serde_json::json!({"responseStatus":429})).is_err());
    }
}
