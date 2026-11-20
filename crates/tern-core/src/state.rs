use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

use crate::error::{CoreError, ErrorKind};

const PRAGMAS: &str = "
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA foreign_keys=ON;
PRAGMA cache_size=-1024;
PRAGMA mmap_size=0;
PRAGMA temp_store=FILE;
PRAGMA busy_timeout=2000;
";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS conversations(
  id TEXT PRIMARY KEY,
  provider_id TEXT NOT NULL,
  provider_session_id TEXT,
  title TEXT NOT NULL,
  created_unix_ms INTEGER NOT NULL,
  updated_unix_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS messages(
  id INTEGER PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  role INTEGER NOT NULL,
  text TEXT NOT NULL,
  attachments_json TEXT NOT NULL DEFAULT '[]',
  created_unix_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS messages_conv ON messages(conversation_id, id);
INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '1');
";

const TITLE_MAX_CHARS: usize = 60;
const UNTITLED_TITLE: &str = "Image question";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    pub id: String,
    pub provider_id: String,
    pub provider_session_id: Option<String>,
    pub title: String,
    pub created_unix_ms: i64,
    pub updated_unix_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    Error,
}

impl Role {
    fn code(self) -> i64 {
        match self {
            Role::User => 0,
            Role::Assistant => 1,
            Role::Error => 2,
        }
    }

    fn from_code(code: i64) -> Result<Self, CoreError> {
        match code {
            0 => Ok(Role::User),
            1 => Ok(Role::Assistant),
            2 => Ok(Role::Error),
            other => Err(CoreError::new(
                ErrorKind::Database,
                format!("unknown message role code {other}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    pub role: Role,
    pub text: String,
    pub attachments: Vec<String>,
    pub created_unix_ms: i64,
}

pub struct StateStore {
    conn: Mutex<Connection>,
}

impl StateStore {
    pub fn open(path: &Path) -> Result<Self, CoreError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(PRAGMAS)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn create_conversation(
        &self,
        provider_id: &str,
        first_prompt: &str,
    ) -> Result<Conversation, CoreError> {
        let now = now_unix_ms();
        let conversation = Conversation {
            id: Uuid::new_v4().hyphenated().to_string(),
            provider_id: provider_id.to_owned(),
            provider_session_id: None,
            title: title_from_prompt(first_prompt),
            created_unix_ms: now,
            updated_unix_ms: now,
        };
        self.connection()?.execute(
            "INSERT INTO conversations(id, provider_id, provider_session_id, title, created_unix_ms, updated_unix_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                conversation.id,
                conversation.provider_id,
                conversation.provider_session_id,
                conversation.title,
                conversation.created_unix_ms,
                conversation.updated_unix_ms
            ],
        )?;
        Ok(conversation)
    }

    pub fn get_conversation(&self, id: &str) -> Result<Option<Conversation>, CoreError> {
        let conn = self.connection()?;
        let conversation = conn
            .query_row(
                "SELECT id, provider_id, provider_session_id, title, created_unix_ms, updated_unix_ms FROM conversations WHERE id = ?1",
                params![id],
                conversation_from_row,
            )
            .optional()?;
        Ok(conversation)
    }

    pub fn set_session_id(&self, id: &str, session_id: &str) -> Result<(), CoreError> {
        let changed = self.connection()?.execute(
            "UPDATE conversations SET provider_session_id = ?1 WHERE id = ?2",
            params![session_id, id],
        )?;
        require_conversation(changed, id)
    }

    pub fn append_message(&self, id: &str, msg: &StoredMessage) -> Result<(), CoreError> {
        let attachments_json = serde_json::to_string(&msg.attachments)?;
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE conversations SET updated_unix_ms = MAX(updated_unix_ms, ?1) WHERE id = ?2",
            params![msg.created_unix_ms, id],
        )?;
        require_conversation(changed, id)?;
        tx.execute(
            "INSERT INTO messages(conversation_id, role, text, attachments_json, created_unix_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id,
                msg.role.code(),
                msg.text,
                attachments_json,
                msg.created_unix_ms
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_conversations(&self, limit: u32) -> Result<Vec<(Conversation, u32)>, CoreError> {
        let conn = self.connection()?;
        let mut statement = conn.prepare(
            "SELECT c.id, c.provider_id, c.provider_session_id, c.title, c.created_unix_ms, c.updated_unix_ms, (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) FROM conversations c ORDER BY c.updated_unix_ms DESC, c.id ASC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], |row| {
            Ok((conversation_from_row(row)?, row.get::<_, i64>(6)?))
        })?;
        let mut summaries = Vec::new();
        for row in rows {
            let (conversation, count) = row?;
            summaries.push((conversation, u32::try_from(count).unwrap_or(u32::MAX)));
        }
        Ok(summaries)
    }

    pub fn messages(&self, id: &str) -> Result<Vec<StoredMessage>, CoreError> {
        let conn = self.connection()?;
        let mut statement = conn.prepare(
            "SELECT role, text, attachments_json, created_unix_ms FROM messages WHERE conversation_id = ?1 ORDER BY id ASC",
        )?;
        let rows = statement.query_map(params![id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        let mut messages = Vec::new();
        for row in rows {
            let (role, text, attachments_json, created_unix_ms) = row?;
            messages.push(StoredMessage {
                role: Role::from_code(role)?,
                text,
                attachments: serde_json::from_str(&attachments_json)?,
                created_unix_ms,
            });
        }
        Ok(messages)
    }

    pub fn prune(&self, keep: u32) -> Result<usize, CoreError> {
        let deleted = self.connection()?.execute(
            "DELETE FROM conversations WHERE id NOT IN (SELECT id FROM conversations ORDER BY updated_unix_ms DESC, id ASC LIMIT ?1)",
            params![keep],
        )?;
        Ok(deleted)
    }

    pub fn shrink_memory(&self) -> Result<(), CoreError> {
        self.connection()?.execute_batch("PRAGMA shrink_memory;")?;
        Ok(())
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, CoreError> {
        self.conn
            .lock()
            .map_err(|_| CoreError::new(ErrorKind::Internal, "state database lock is poisoned"))
    }
}

fn conversation_from_row(row: &Row<'_>) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: row.get(0)?,
        provider_id: row.get(1)?,
        provider_session_id: row.get(2)?,
        title: row.get(3)?,
        created_unix_ms: row.get(4)?,
        updated_unix_ms: row.get(5)?,
    })
}

fn require_conversation(changed: usize, id: &str) -> Result<(), CoreError> {
    if changed == 0 {
        Err(CoreError::new(
            ErrorKind::InvalidInput,
            format!("unknown conversation id {id}"),
        ))
    } else {
        Ok(())
    }
}

fn title_from_prompt(prompt: &str) -> String {
    let single_line: String = prompt
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
        .chars()
        .take(TITLE_MAX_CHARS)
        .collect();
    let trimmed = single_line.trim_end();
    if trimmed.is_empty() {
        UNTITLED_TITLE.to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}
