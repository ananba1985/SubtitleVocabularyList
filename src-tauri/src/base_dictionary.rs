use crate::{error::AppError, explanations::Explanation, vocabulary::normalize};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Deserialize;
use std::{collections::HashMap, path::Path, sync::Mutex};

#[derive(Default)]
pub(crate) struct BaseDictionary {
    entries: HashMap<String, String>,
    aliases: HashMap<String, String>,
    source: String,
    ecdict: Mutex<Option<Connection>>,
    supplements: Supplements,
    warnings: Mutex<Vec<String>>,
}

#[derive(Default, Deserialize)]
struct Supplements {
    definitions: HashMap<String, Definition>,
    variants: HashMap<String, Variant>,
    #[serde(default)]
    ocr_variants: HashMap<String, Variant>,
}

#[derive(Deserialize)]
struct Definition { meaning: String, notes: String }

#[derive(Deserialize)]
struct Variant { target: String, kind: String }

fn dictionary_key(text: &str) -> String {
    normalize(text).replace(['’', '‘', 'ʼ'], "'")
}

impl BaseDictionary {
    pub fn load(root: &Path) -> Self {
        let mut dictionary = Self::default();
        match serde_json::from_str(include_str!("../resources/dictionary-supplements.json")) {
            Ok(supplements) => dictionary.supplements = supplements,
            Err(error) => dictionary.warn("内置词义补充", &error.to_string()),
        }
        let path = root.join("dictionaries/base.sqlite3");
        if path.exists() {
            match Self::load_google(&path) {
                Ok((entries, aliases, source)) => {
                    dictionary.entries = entries;
                    dictionary.aliases = aliases;
                    dictionary.source = source;
                }
                Err(error) => dictionary.warn("Google 基础词库（base.sqlite3）", &error.message),
            }
        }
        let ecdict = root.join("dictionaries/ecdict.sqlite3");
        if ecdict.exists() {
            let loaded = (|| -> Result<Connection, AppError> {
                let connection = Connection::open_with_flags(ecdict, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
                // Validate the required schema without loading the large dictionary.
                connection.prepare_cached(ECDICT_LOOKUP)?;
                Ok(connection)
            })();
            match loaded {
                Ok(connection) => dictionary.ecdict = Mutex::new(Some(connection)),
                Err(error) => dictionary.warn("ECDICT 基础词库（ecdict.sqlite3）", &error.message),
            }
        }
        dictionary
    }

    fn load_google(path: &Path) -> Result<(HashMap<String, String>, HashMap<String, String>, String), AppError> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut entries = HashMap::new();
        let mut aliases = HashMap::new();
        let mut statement = connection.prepare("SELECT word,meaning FROM dictionary_entries")?;
        for row in statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (word, meaning) = row?;
            if meaning.chars().any(|c| ('\u{3400}'..='\u{9fff}').contains(&c)) {
                entries.insert(dictionary_key(&word), meaning);
            }
        }
        let mut statement = connection.prepare("SELECT form,lemma FROM dictionary_aliases")?;
        for row in statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (form, lemma) = row?;
            let lemma = dictionary_key(&lemma);
            if entries.contains_key(&lemma) {
                aliases.insert(dictionary_key(&form), lemma);
            }
        }
        let source = connection.query_row(
            "SELECT value FROM metadata WHERE key='source'",
            [],
            |row| row.get(0),
        )?;
        Ok((entries, aliases, source))
    }

    fn warn(&self, resource: &str, reason: &str) {
        if let Ok(mut warnings) = self.warnings.lock() {
            let warning = format!("{resource}暂不可用：{reason}。可继续使用其他词库和个人资料；请检查基础词库资源，修复后重新打开程序。");
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        }
    }

    pub fn warnings(&self) -> Vec<String> {
        self.warnings.lock().map(|warnings| warnings.clone()).unwrap_or_default()
    }

    pub fn explanation(&self, text: &str) -> Result<Option<Explanation>, AppError> {
        let original = normalize(text);
        let key = dictionary_key(text);
        if let Some(mut value) = self.lookup(&key)? {
            if original != key {
                value.notes.push_str(&format!("；撇号字形规范化 {} → {}", original, key));
            }
            return Ok(Some(value));
        }
        if let Some(variant) = self.supplements.variants.get(&key)
            && let Some(mut value) = self.lookup(&variant.target)?
        {
            value.notes.push_str(&format!("；{} {} → {}", variant.kind, original, variant.target));
            return Ok(Some(value));
        }
        Ok(None)
    }

    // These corrections have been checked against subtitle pixels. They are
    // deliberately separate from ordinary dictionary aliases; the caller must
    // establish that this target actually came from a PGS OCR occurrence.
    pub(crate) fn ocr_explanation(&self, text: &str) -> Result<Option<Explanation>, AppError> {
        let Some(variant) = self.supplements.ocr_variants.get(&dictionary_key(text)) else {
            return Ok(None);
        };
        Ok(self.lookup(&variant.target)?.map(|mut value| {
            value.notes.push_str(&format!("；{}：{} → {}；原字幕文字保留", variant.kind, text, variant.target));
            value
        }))
    }

    fn lookup(&self, key: &str) -> Result<Option<Explanation>, AppError> {
        if let Some(word) = self.entries.get_key_value(key).map(|(word, _)| word)
            .or_else(|| self.aliases.get(key))
        {
            return Ok(Some(Self::value(key, word, self.entries[word].clone(), &self.source)));
        }
        if let Some(value) = self.supplements.definitions.get(key) {
            return Ok(Some(Explanation {
                meaning: value.meaning.clone(),
                translation: String::new(),
                notes: format!("基础词义补充（自拟中文）：{}", value.notes),
            }));
        }
        let Ok(mut ecdict) = self.ecdict.lock() else {
            self.warn("ECDICT 基础词库（ecdict.sqlite3）", "读取状态不可用");
            return Ok(None);
        };
        let Some(connection) = ecdict.as_ref() else { return Ok(None); };
        let result = (|| -> Result<_, rusqlite::Error> {
            let mut query = connection.prepare_cached(ECDICT_LOOKUP)?;
            query.query_row([key], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }).optional()
        })();
        match result {
            Ok(row) => Ok(row.map(|(word, meaning)| Self::value(key, &word, meaning, "ECDICT"))),
            Err(error) => {
                *ecdict = None;
                self.warn("ECDICT 基础词库（ecdict.sqlite3）", &error.to_string());
                Ok(None)
            }
        }
    }

    fn value(key: &str, word: &str, meaning: String, source: &str) -> Explanation {
        Explanation {
            meaning,
            translation: String::new(),
            notes: if word == key {
                format!("基础词库：{}", source)
            } else {
                format!("基础词库：{}；词形 {} → {}", source, key, word)
            },
        }
    }
}

const ECDICT_LOOKUP: &str = "SELECT word,meaning FROM (
    SELECT word,meaning,0 priority FROM dictionary_entries WHERE word=?1
    UNION ALL
    SELECT e.word,e.meaning,1 priority FROM dictionary_aliases a
      JOIN dictionary_entries e ON e.word=a.lemma WHERE a.form=?1
    ) ORDER BY priority LIMIT 1";
