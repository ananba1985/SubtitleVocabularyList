use crate::{error::AppError, explanations::Explanation, vocabulary::normalize};
use rusqlite::{Connection, OpenFlags};
use std::{collections::HashMap, path::Path};

#[derive(Default)]
pub(crate) struct BaseDictionary {
    entries: HashMap<String, String>,
    aliases: HashMap<String, String>,
    source: String,
}

impl BaseDictionary {
    pub fn load(root: &Path) -> Result<Self, AppError> {
        let path = root.join("dictionaries/base.sqlite3");
        if !path.exists() {
            return Ok(Self::default());
        }
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut dictionary = Self::default();
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

    pub fn explanation(&self, text: &str) -> Option<Explanation> {
        let key = normalize(text);
        let word = if self.entries.contains_key(&key) {
            &key
        } else {
            self.aliases.get(&key)?
        };
        Some(Explanation {
            meaning: self.entries.get(word)?.clone(),
            translation: String::new(),
            notes: if word == &key {
                format!("基础词库：{}", self.source)
            } else {
                format!("基础词库：{}；词形 {} → {}", self.source, key, word)
            },
        })
    }
}
