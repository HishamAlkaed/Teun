use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageFeedback {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_answer: Option<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback: Option<MessageFeedback>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge_result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    #[serde(default)]
    pub messages: Vec<StoredMessage>,
}

/// Summary returned by list (without full messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
}

#[derive(Clone)]
pub struct SessionStore {
    pool: PgPool,
}

impl SessionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<SessionSummary>> {
        let rows = sqlx::query(
            "SELECT id, title, created_at, last_active FROM sessions ORDER BY last_active DESC LIMIT 100",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| SessionSummary {
                id: r.get("id"),
                title: r.get("title"),
                created_at: r.get("created_at"),
                last_active: r.get("last_active"),
            })
            .collect())
    }

    pub async fn get(&self, id: &str) -> Result<Option<StoredSession>> {
        let session_row =
            sqlx::query("SELECT id, title, created_at, last_active FROM sessions WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;

        let session_row = match session_row {
            Some(r) => r,
            None => return Ok(None),
        };

        let msg_rows = sqlx::query(
            "SELECT id, role, content, structured_answer, timestamp, timeline, \
                    judge_result, feedback_status, feedback_comment, feedback_created_at \
             FROM messages WHERE session_id = $1 ORDER BY timestamp",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;

        let messages = msg_rows
            .iter()
            .map(|r| {
                let fb_status: Option<String> = r.get("feedback_status");
                let feedback = fb_status.map(|status| MessageFeedback {
                    status,
                    comment: r.get("feedback_comment"),
                    created_at: r
                        .get::<Option<DateTime<Utc>>, _>("feedback_created_at")
                        .unwrap_or_else(Utc::now),
                });
                StoredMessage {
                    id: r.get("id"),
                    role: r.get("role"),
                    content: r.get("content"),
                    structured_answer: r.get("structured_answer"),
                    timestamp: r.get("timestamp"),
                    timeline: r.get("timeline"),
                    feedback,
                    judge_result: r.get("judge_result"),
                }
            })
            .collect();

        Ok(Some(StoredSession {
            id: session_row.get("id"),
            title: session_row.get("title"),
            created_at: session_row.get("created_at"),
            last_active: session_row.get("last_active"),
            messages,
        }))
    }

    pub async fn append_messages(
        &self,
        session_id: &str,
        title: &str,
        new_messages: Vec<StoredMessage>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO sessions (id, title, created_at, last_active) \
             VALUES ($1, $2, now(), now()) \
             ON CONFLICT (id) DO UPDATE SET last_active = now()",
        )
        .bind(session_id)
        .bind(title)
        .execute(&mut *tx)
        .await?;

        for msg in &new_messages {
            let (fb_status, fb_comment, fb_created_at): (
                Option<&str>,
                Option<&str>,
                Option<DateTime<Utc>>,
            ) = match &msg.feedback {
                Some(f) => (
                    Some(f.status.as_str()),
                    f.comment.as_deref(),
                    Some(f.created_at),
                ),
                None => (None, None, None),
            };
            sqlx::query(
                "INSERT INTO messages (id, session_id, role, content, structured_answer, \
                                      timestamp, timeline, judge_result, \
                                      feedback_status, feedback_comment, feedback_created_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            )
            .bind(&msg.id)
            .bind(session_id)
            .bind(&msg.role)
            .bind(&msg.content)
            .bind(&msg.structured_answer)
            .bind(msg.timestamp)
            .bind(&msg.timeline)
            .bind(&msg.judge_result)
            .bind(fb_status)
            .bind(fb_comment)
            .bind(fb_created_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn set_message_feedback(
        &self,
        session_id: &str,
        message_id: &str,
        feedback: MessageFeedback,
    ) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE messages SET feedback_status = $1, feedback_comment = $2, feedback_created_at = $3 \
             WHERE id = $4 AND session_id = $5",
        )
        .bind(&feedback.status)
        .bind(&feedback.comment)
        .bind(feedback.created_at)
        .bind(message_id)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn cleanup_expired(&self, retention_days: i64) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM sessions WHERE last_active < now() - make_interval(days => $1::int)",
        )
        .bind(retention_days as i32)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}
