use forge_core::Message;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use tracing::{debug, error, info};

/// Persistent session storage using SQLite
pub struct SessionStore {
    conn: Connection,
}

/// A saved session summary for the session browser
#[derive(Debug, Clone)]
pub struct SavedSession {
    pub id: String,
    pub model: String,
    pub provider: String,
    pub project: Option<String>,
    pub started_at: String,
    pub message_count: usize,
    pub total_tokens: usize,
}

impl SessionStore {
    pub fn open() -> Result<Self, String> {
        let path = db_path();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create dir: {e}"))?;
        }

        let conn = Connection::open(&path)
            .map_err(|e| format!("Failed to open session DB: {e}"))?;

        // Create tables
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                model TEXT NOT NULL,
                provider TEXT NOT NULL,
                project TEXT,
                started_at TEXT NOT NULL,
                total_input_tokens INTEGER DEFAULT 0,
                total_output_tokens INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                message_json TEXT NOT NULL,
                seq INTEGER NOT NULL,
                created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session
                ON messages(session_id, seq);",
        )
        .map_err(|e| format!("Failed to create tables: {e}"))?;

        info!("Session store opened at {}", path.display());
        Ok(Self { conn })
    }

    /// Save a session to the database
    #[allow(clippy::too_many_arguments)]
    pub fn save_session(
        &self,
        id: &str,
        model: &str,
        provider: &str,
        project: Option<&str>,
        started_at: &str,
        messages: &[Message],
        input_tokens: usize,
        output_tokens: usize,
    ) -> Result<(), String> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| format!("Transaction failed: {e}"))?;

        // Upsert session
        tx.execute(
            "INSERT OR REPLACE INTO sessions (id, model, provider, project, started_at, total_input_tokens, total_output_tokens)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, model, provider, project, started_at, input_tokens, output_tokens],
        )
        .map_err(|e| format!("Failed to save session: {e}"))?;

        // Delete old messages for this session
        tx.execute("DELETE FROM messages WHERE session_id = ?1", params![id])
            .map_err(|e| format!("Failed to clear messages: {e}"))?;

        // Insert messages
        for (seq, msg) in messages.iter().enumerate() {
            let json = serde_json::to_string(msg)
                .map_err(|e| format!("Failed to serialize message: {e}"))?;
            tx.execute(
                "INSERT INTO messages (session_id, message_json, seq) VALUES (?1, ?2, ?3)",
                params![id, json, seq],
            )
            .map_err(|e| format!("Failed to save message: {e}"))?;
        }

        tx.commit()
            .map_err(|e| format!("Commit failed: {e}"))?;

        debug!(
            "Saved session {id} with {} messages",
            messages.len()
        );
        Ok(())
    }

    /// Load messages for a session
    pub fn load_messages(&self, session_id: &str) -> Result<Vec<Message>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT message_json FROM messages WHERE session_id = ?1 ORDER BY seq")
            .map_err(|e| format!("Prepare failed: {e}"))?;

        let messages: Vec<Message> = stmt
            .query_map(params![session_id], |row| {
                let json: String = row.get(0)?;
                Ok(json)
            })
            .map_err(|e| format!("Query failed: {e}"))?
            .filter_map(|r| {
                r.ok().and_then(|json| {
                    serde_json::from_str(&json)
                        .map_err(|e| error!("Failed to deserialize message: {e}"))
                        .ok()
                })
            })
            .collect();

        debug!(
            "Loaded {} messages for session {session_id}",
            messages.len()
        );
        Ok(messages)
    }

    /// List recent sessions
    pub fn list_sessions(&self, limit: usize) -> Result<Vec<SavedSession>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT s.id, s.model, s.provider, s.project, s.started_at,
                        s.total_input_tokens, s.total_output_tokens,
                        COUNT(m.id) as msg_count
                 FROM sessions s
                 LEFT JOIN messages m ON m.session_id = s.id
                 GROUP BY s.id
                 ORDER BY s.created_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| format!("Prepare failed: {e}"))?;

        let sessions: Vec<SavedSession> = stmt
            .query_map(params![limit], |row| {
                let input_tokens: usize = row.get(5)?;
                let output_tokens: usize = row.get(6)?;
                Ok(SavedSession {
                    id: row.get(0)?,
                    model: row.get(1)?,
                    provider: row.get(2)?,
                    project: row.get(3)?,
                    started_at: row.get(4)?,
                    message_count: row.get(7)?,
                    total_tokens: input_tokens + output_tokens,
                })
            })
            .map_err(|e| format!("Query failed: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(sessions)
    }

    /// Get the most recent session ID
    pub fn last_session_id(&self) -> Option<String> {
        self.conn
            .query_row(
                "SELECT id FROM sessions ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok()
    }
}

fn db_path() -> PathBuf {
    dirs::data_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forge")
        .join("sessions.db")
}
