//! SQLite-backed journal store.

use crate::types::{JournalEntry, JournalError, JournalStats, Sentiment};
use jiff::{tz::TimeZone, Timestamp};
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
        created_at: Timestamp,
    ) -> Result<JournalEntry, JournalError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| JournalError::Storage(format!("mutex poisoned: {e}")))?;
        let tag_csv = tags.join(",");
        conn.execute(
            "INSERT INTO journal_entries (created_at, text, sentiment, tags) VALUES (?, ?, ?, ?)",
            params![to_rfc3339(created_at), text, sentiment.as_str(), tag_csv],
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
        from: Option<Timestamp>,
        to: Option<Timestamp>,
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
            args.push(to_rfc3339(f));
        }
        if let Some(t) = to {
            sql.push_str(" AND created_at < ?");
            args.push(to_rfc3339(t));
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
            let parsed_created = created
                .parse::<Timestamp>()
                .map_err(|e| JournalError::Storage(format!("invalid created_at: {e}")))?;
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
        from: Option<Timestamp>,
        to: Option<Timestamp>,
    ) -> Result<JournalStats, JournalError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| JournalError::Storage(format!("mutex poisoned: {e}")))?;
        let mut sql = String::from("SELECT sentiment, COUNT(*) FROM journal_entries WHERE 1=1");
        let mut args: Vec<String> = Vec::new();
        if let Some(f) = from {
            sql.push_str(" AND created_at >= ?");
            args.push(to_rfc3339(f));
        }
        if let Some(t) = to {
            sql.push_str(" AND created_at < ?");
            args.push(to_rfc3339(t));
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

/// RFC 3339 in UTC with a `+00:00` offset and 0, 3, 6 or 9 fractional digits.
///
/// This is the exact text persisted in `created_at`, so rows written before and
/// after the move to `jiff` stay comparable as plain strings.
fn to_rfc3339(ts: Timestamp) -> String {
    let zoned = ts.to_zoned_datetime(TimeZone::UTC);
    let nanos = zoned.subsec_nanosecond();
    let fraction = if nanos == 0 {
        String::new()
    } else if nanos % 1_000_000 == 0 {
        format!(".{:03}", nanos / 1_000_000)
    } else if nanos % 1_000 == 0 {
        format!(".{:06}", nanos / 1_000)
    } else {
        format!(".{nanos:09}")
    };
    format!("{}{fraction}+00:00", zoned.strftime("%Y-%m-%dT%H:%M:%S"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> Timestamp {
        s.parse().expect("timestamp")
    }

    #[test]
    fn rfc3339_uses_plus_zero_offset_without_fraction_for_whole_seconds() {
        assert_eq!(
            to_rfc3339(ts("2026-04-18T12:00:00Z")),
            "2026-04-18T12:00:00+00:00"
        );
    }

    #[test]
    fn rfc3339_groups_fractional_digits_in_threes() {
        assert_eq!(
            to_rfc3339(ts("2026-04-18T12:00:00.5Z")),
            "2026-04-18T12:00:00.500+00:00"
        );
        assert_eq!(
            to_rfc3339(ts("2026-04-18T12:00:00.000123Z")),
            "2026-04-18T12:00:00.000123+00:00"
        );
        assert_eq!(
            to_rfc3339(ts("2026-04-18T12:00:00.123456789Z")),
            "2026-04-18T12:00:00.123456789+00:00"
        );
    }

    #[test]
    fn stored_text_round_trips_through_the_store() {
        let store = SqliteJournalStore::open_in_memory().expect("open");
        let at = ts("2026-04-18T12:00:00.250Z");
        let entry = store
            .insert("hello", Sentiment::Neutral, &[], at)
            .expect("insert");
        assert_eq!(entry.created_at, at);
        let rows = store.recall(None, None, None, 10).expect("recall");
        assert_eq!(rows[0].created_at, at);
    }
}
