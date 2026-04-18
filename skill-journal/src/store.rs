//! SQLite-backed journal store.

use crate::types::{JournalEntry, JournalError, JournalStats, Sentiment};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

pub struct SqliteJournalStore {
    conn: Mutex<Connection>,
}

impl SqliteJournalStore {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, JournalError> {
        let conn = Connection::open(path).map_err(|e| JournalError::Storage(e.to_string()))?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, JournalError> {
        let conn =
            Connection::open_in_memory().map_err(|e| JournalError::Storage(e.to_string()))?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn init_schema(conn: &Connection) -> Result<(), JournalError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS journal_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL,
                text TEXT NOT NULL,
                sentiment TEXT NOT NULL,
                tags TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_journal_created_at \
                ON journal_entries (created_at);",
        )
        .map_err(|e| JournalError::Storage(e.to_string()))
    }

    pub fn insert(
        &self,
        text: &str,
        sentiment: Sentiment,
        tags: &[String],
        created_at: DateTime<Utc>,
    ) -> Result<JournalEntry, JournalError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| JournalError::Storage(format!("mutex poisoned: {e}")))?;
        let tag_csv = tags.join(",");
        conn.execute(
            "INSERT INTO journal_entries (created_at, text, sentiment, tags) VALUES (?, ?, ?, ?)",
            params![created_at.to_rfc3339(), text, sentiment.as_str(), tag_csv],
        )
        .map_err(|e| JournalError::Storage(e.to_string()))?;
        let id = conn.last_insert_rowid();
        Ok(JournalEntry {
            id,
            created_at,
            text: text.to_string(),
            sentiment,
            tags: tags.to_vec(),
        })
    }

    pub fn recall(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        contains: Option<&str>,
        limit: usize,
    ) -> Result<Vec<JournalEntry>, JournalError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| JournalError::Storage(format!("mutex poisoned: {e}")))?;
        let mut sql = String::from(
            "SELECT id, created_at, text, sentiment, tags FROM journal_entries WHERE 1=1",
        );
        let mut args: Vec<String> = Vec::new();
        if let Some(f) = from {
            sql.push_str(" AND created_at >= ?");
            args.push(f.to_rfc3339());
        }
        if let Some(t) = to {
            sql.push_str(" AND created_at < ?");
            args.push(t.to_rfc3339());
        }
        if let Some(c) = contains {
            if !c.trim().is_empty() {
                sql.push_str(" AND text LIKE ?");
                args.push(format!("%{}%", c.trim()));
            }
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        args.push(limit.to_string());
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| JournalError::Storage(e.to_string()))?;
        let params_owned: Vec<&dyn rusqlite::ToSql> =
            args.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let mut rows = stmt
            .query(params_owned.as_slice())
            .map_err(|e| JournalError::Storage(e.to_string()))?;
        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|e| JournalError::Storage(e.to_string()))?
        {
            let id: i64 = row
                .get(0)
                .map_err(|e| JournalError::Storage(e.to_string()))?;
            let created: String = row
                .get(1)
                .map_err(|e| JournalError::Storage(e.to_string()))?;
            let text: String = row
                .get(2)
                .map_err(|e| JournalError::Storage(e.to_string()))?;
            let sentiment_str: String = row
                .get(3)
                .map_err(|e| JournalError::Storage(e.to_string()))?;
            let tags_str: String = row
                .get(4)
                .map_err(|e| JournalError::Storage(e.to_string()))?;
            let parsed_created = DateTime::parse_from_rfc3339(&created)
                .map_err(|e| JournalError::Storage(format!("invalid created_at: {e}")))?
                .with_timezone(&Utc);
            let sentiment = Sentiment::parse(&sentiment_str).unwrap_or(Sentiment::Neutral);
            let tags = if tags_str.is_empty() {
                Vec::new()
            } else {
                tags_str.split(',').map(str::to_string).collect()
            };
            out.push(JournalEntry {
                id,
                created_at: parsed_created,
                text,
                sentiment,
                tags,
            });
        }
        Ok(out)
    }

    pub fn stats(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> Result<JournalStats, JournalError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| JournalError::Storage(format!("mutex poisoned: {e}")))?;
        let mut sql = String::from("SELECT sentiment, COUNT(*) FROM journal_entries WHERE 1=1");
        let mut args: Vec<String> = Vec::new();
        if let Some(f) = from {
            sql.push_str(" AND created_at >= ?");
            args.push(f.to_rfc3339());
        }
        if let Some(t) = to {
            sql.push_str(" AND created_at < ?");
            args.push(t.to_rfc3339());
        }
        sql.push_str(" GROUP BY sentiment");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| JournalError::Storage(e.to_string()))?;
        let params_owned: Vec<&dyn rusqlite::ToSql> =
            args.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let mut rows = stmt
            .query(params_owned.as_slice())
            .map_err(|e| JournalError::Storage(e.to_string()))?;
        let mut pos = 0usize;
        let mut neu = 0usize;
        let mut neg = 0usize;
        while let Some(row) = rows
            .next()
            .map_err(|e| JournalError::Storage(e.to_string()))?
        {
            let sentiment_str: String = row
                .get(0)
                .map_err(|e| JournalError::Storage(e.to_string()))?;
            let count: i64 = row
                .get(1)
                .map_err(|e| JournalError::Storage(e.to_string()))?;
            match Sentiment::parse(&sentiment_str).unwrap_or(Sentiment::Neutral) {
                Sentiment::Positive => pos = count as usize,
                Sentiment::Neutral => neu = count as usize,
                Sentiment::Negative => neg = count as usize,
            }
        }
        Ok(JournalStats {
            total_entries: pos + neu + neg,
            positive: pos,
            neutral: neu,
            negative: neg,
        })
    }
}
