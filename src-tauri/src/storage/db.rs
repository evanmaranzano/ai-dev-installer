use std::path::{Path, PathBuf};

use crate::error::AppError;
use rusqlite::{params, Connection};

const MIGRATION_SQL: &str = include_str!("../../migrations/0001_init.sql");

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryRow {
    pub role: String,
    pub content: String,
}

pub struct HistoryDatabase {
    connection: Connection,
}

impl HistoryDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| AppError {
                code: "history_db_open_failed".to_string(),
                message: "Failed to prepare history database directory".to_string(),
                details: Some(error.to_string()),
            })?;
        }

        let connection = Connection::open(path).map_err(|error| AppError {
            code: "history_db_open_failed".to_string(),
            message: "Failed to open history database".to_string(),
            details: Some(error.to_string()),
        })?;

        connection
            .execute_batch(MIGRATION_SQL)
            .map_err(|error| AppError {
                code: "history_db_migration_failed".to_string(),
                message: "Failed to apply history database migration".to_string(),
                details: Some(error.to_string()),
            })?;

        Ok(Self { connection })
    }

    pub fn open_in_memory() -> Result<Self, AppError> {
        let connection = Connection::open_in_memory().map_err(|error| AppError {
            code: "history_db_open_failed".to_string(),
            message: "Failed to open in-memory history database".to_string(),
            details: Some(error.to_string()),
        })?;

        connection
            .execute_batch(MIGRATION_SQL)
            .map_err(|error| AppError {
                code: "history_db_migration_failed".to_string(),
                message: "Failed to apply history database migration".to_string(),
                details: Some(error.to_string()),
            })?;

        Ok(Self { connection })
    }

    pub fn insert_chat_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
    ) -> Result<(), AppError> {
        let now = chrono_like_timestamp();
        self.connection
            .execute(
                "INSERT OR IGNORE INTO chat_sessions (id, title, created_at) VALUES (?1, ?2, ?3)",
                params![session_id, "默认会话", now],
            )
            .map_err(|error| AppError {
                code: "history_insert_failed".to_string(),
                message: "Failed to ensure chat session".to_string(),
                details: Some(error.to_string()),
            })?;

        self.connection
            .execute(
                "INSERT INTO chat_messages (session_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![session_id, role, content, now],
            )
            .map_err(|error| AppError {
                code: "history_insert_failed".to_string(),
                message: "Failed to insert chat history row".to_string(),
                details: Some(error.to_string()),
            })?;

        Ok(())
    }

    pub fn list_messages(&self, session_id: &str) -> Result<Vec<ChatHistoryRow>, AppError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT role, content FROM chat_messages WHERE session_id = ?1 ORDER BY id ASC",
            )
            .map_err(|error| AppError {
                code: "history_read_failed".to_string(),
                message: "Failed to prepare chat history query".to_string(),
                details: Some(error.to_string()),
            })?;

        let rows = statement
            .query_map(params![session_id], |row| {
                Ok(ChatHistoryRow {
                    role: row.get(0)?,
                    content: row.get(1)?,
                })
            })
            .map_err(|error| AppError {
                code: "history_read_failed".to_string(),
                message: "Failed to query chat history".to_string(),
                details: Some(error.to_string()),
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError {
                code: "history_read_failed".to_string(),
                message: "Failed to decode chat history rows".to_string(),
                details: Some(error.to_string()),
            })?;

        Ok(rows)
    }
}

pub fn default_history_db_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("AI Dev Installer").join("history.sqlite3")
}

fn chrono_like_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
