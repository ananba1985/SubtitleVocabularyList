use crate::{error::AppError, explanations::Explanation, vocabulary::normalize};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::{collections::HashMap, path::Path, sync::Mutex};

#[derive(Default)]
pub(crate) struct BaseDictionary {
    entries: HashMap<String, String>,
    aliases: HashMap<String, String>,
    source: String,
    ecdict: Option<Mutex<Connection>>,
}

impl BaseDictionary {
    pub fn load(root: &Path) -> Result<Self, AppError> {
        let path = root.join("dictionaries/base.sqlite3");
        let mut dictionary = Self::default();
        let ecdict = root.join("dictionaries/ecdict.sqlite3");
        if ecdict.exists() {
            dictionary.ecdict = Some(Mutex::new(Connection::open_with_flags(
                ecdict,
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?));
        }
        if !path.exists() {
            return Ok(dictionary);
        }
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut statement = connection.prepare("SELECT word,meaning FROM dictionary_entries")?;
        for row in statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (word, meaning) = row?;
            if meaning.chars().any(|c| ('\u{3400}'..='\u{9fff}').contains(&c)) {
                dictionary.entries.insert(normalize(&word), meaning);
            }
        }
        let mut statement = connection.prepare("SELECT form,lemma FROM dictionary_aliases")?;
        for row in statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (form, lemma) = row?;
            let lemma = normalize(&lemma);
            if dictionary.entries.contains_key(&lemma) {
                dictionary.aliases.insert(normalize(&form), lemma);
            }
        }
        dictionary.source = connection.query_row(
            "SELECT value FROM metadata WHERE key='source'",
            [],
            |row| row.get(0),
        )?;
        Ok(dictionary)
    }

    pub fn explanation(&self, text: &str) -> Result<Option<Explanation>, AppError> {
        let key = normalize(text);
        if let Some(word) = self.entries.get_key_value(&key).map(|(word, _)| word)
            .or_else(|| self.aliases.get(&key))
        {
            return Ok(Some(Self::value(&key, word, self.entries[word].clone(), &self.source)));
        }
        let Some(ecdict) = &self.ecdict else {
            return Ok(None);
        };
        let connection = ecdict.lock()
            .map_err(|_| AppError::new("database_unavailable", "ECDICT 词库当前不可用。"))?;
        let mut query = connection.prepare_cached(
            "SELECT word,meaning FROM (
                SELECT word,meaning,0 priority FROM dictionary_entries WHERE word=?1
                UNION ALL
                SELECT e.word,e.meaning,1 priority FROM dictionary_aliases a
                  JOIN dictionary_entries e ON e.word=a.lemma WHERE a.form=?1
             ) ORDER BY priority LIMIT 1")?;
        let row = query.query_row([&key], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).optional()?;
        Ok(row.map(|(word, meaning)| Self::value(&key, &word, meaning, "ECDICT")))
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
