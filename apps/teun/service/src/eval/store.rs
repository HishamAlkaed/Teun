use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::{PgPool, Row};

use super::types::{EvalResult, EvalRun, EvalRunStatus, EvalSummary, TestQuestion};

const APP_ID: &str = "teun";

#[derive(Clone)]
pub struct EvalStore {
    pool: PgPool,
}

impl EvalStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_questions(&self) -> Result<Vec<TestQuestion>> {
        let rows = sqlx::query(
            "SELECT id, app_id, question, expected_category, \
                    expected_key_points, description, points, created_at, updated_at \
             FROM test_questions WHERE app_id = $1 ORDER BY created_at",
        )
        .bind(APP_ID)
        .fetch_all(&self.pool)
        .await
        .context("Failed to list questions")?;

        Ok(rows
            .iter()
            .map(|r| TestQuestion {
                id: r.get("id"),
                app_id: r.get("app_id"),
                question: r.get("question"),
                expected_category: r.get("expected_category"),
                expected_key_points: r.get("expected_key_points"),
                description: r.get("description"),
                points: r.get::<i16, _>("points") as u16,
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn create_question(&self, question: &TestQuestion) -> Result<()> {
        sqlx::query(
            "INSERT INTO test_questions (id, app_id, question, expected_category, \
                                         expected_key_points, description, points, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(&question.id)
        .bind(&question.app_id)
        .bind(&question.question)
        .bind(&question.expected_category)
        .bind(&question.expected_key_points)
        .bind(&question.description)
        .bind(question.points as i16)
        .bind(question.created_at)
        .bind(question.updated_at)
        .execute(&self.pool)
        .await
        .context("Failed to create question")?;
        Ok(())
    }

    pub async fn update_question(
        &self,
        id: &str,
        question_text: &str,
        expected_category: Option<&str>,
        expected_key_points: &[String],
        description: Option<&str>,
        points: u16,
    ) -> Result<bool> {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE test_questions SET question = $1, expected_category = $2, \
             expected_key_points = $3, description = $4, points = $5, updated_at = $6 WHERE id = $7",
        )
        .bind(question_text)
        .bind(expected_category)
        .bind(expected_key_points)
        .bind(description)
        .bind(points as i16)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to update question")?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_question(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM test_questions WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete question")?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn create_run(&self, run: &EvalRun) -> Result<()> {
        sqlx::query(
            "INSERT INTO eval_runs (id, app_id, status, started_by, created_at, \
                                    total_questions, completed_questions) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&run.id)
        .bind(&run.app_id)
        .bind(run.status.as_str())
        .bind(&run.started_by)
        .bind(run.created_at)
        .bind(run.total_questions as i32)
        .bind(run.completed_questions as i32)
        .execute(&self.pool)
        .await
        .context("Failed to create eval run")?;
        Ok(())
    }

    pub async fn set_status(&self, run_id: &str, status: EvalRunStatus) -> Result<()> {
        sqlx::query("UPDATE eval_runs SET status = $1 WHERE id = $2")
            .bind(status.as_str())
            .bind(run_id)
            .execute(&self.pool)
            .await
            .context("Failed to update run status")?;
        Ok(())
    }

    pub async fn push_result(&self, run_id: &str, result: &EvalResult) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO eval_results (id, run_id, question_id, question, expected_category, \
                                        expected_key_points, actual_answer, actual_rationale, \
                                        actual_category, actual_sources, chat_session_id, \
                                        judge_score, judge_verdict, judge_reasoning, \
                                        evaluated_at, error) \
             VALUES (gen_random_uuid()::text, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, \
                     $11, $12, $13, $14, $15)",
        )
        .bind(run_id)
        .bind(&result.question_id)
        .bind(&result.question)
        .bind(&result.expected_category)
        .bind(&result.expected_key_points)
        .bind(&result.actual_answer)
        .bind(&result.actual_rationale)
        .bind(&result.actual_category)
        .bind(&result.actual_sources)
        .bind(&result.chat_session_id)
        .bind(result.judge_score.map(|s| s as i16))
        .bind(&result.judge_verdict)
        .bind(&result.judge_reasoning)
        .bind(result.evaluated_at)
        .bind(&result.error)
        .execute(&mut *tx)
        .await
        .context("Failed to insert eval result")?;

        sqlx::query(
            "UPDATE eval_runs SET completed_questions = completed_questions + 1 WHERE id = $1",
        )
        .bind(run_id)
        .execute(&mut *tx)
        .await
        .context("Failed to increment completed_questions")?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn complete_run(&self, run_id: &str, summary: &EvalSummary) -> Result<()> {
        let summary_json =
            serde_json::to_value(summary).context("Failed to serialize summary")?;
        let now = Utc::now();
        sqlx::query(
            "UPDATE eval_runs SET status = 'completed', summary = $1, completed_at = $2 WHERE id = $3",
        )
        .bind(summary_json)
        .bind(now)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .context("Failed to complete eval run")?;
        Ok(())
    }

    pub async fn fail_run(&self, run_id: &str, error: &str) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            "UPDATE eval_runs SET status = 'failed', error = $1, completed_at = $2 WHERE id = $3",
        )
        .bind(error)
        .bind(now)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .context("Failed to fail eval run")?;
        Ok(())
    }

    pub async fn get_run_status(&self, run_id: &str) -> Result<Option<EvalRunStatus>> {
        let row = sqlx::query("SELECT status FROM eval_runs WHERE id = $1")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to get run status")?;
        Ok(row.map(|r| EvalRunStatus::from_str(r.get("status"))))
    }

    pub async fn stop_run(&self, run_id: &str) -> Result<bool> {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE eval_runs SET status = 'stopped', completed_at = $1 \
             WHERE id = $2 AND status IN ('pending', 'running')",
        )
        .bind(now)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .context("Failed to stop eval run")?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_run(&self, run_id: &str) -> Result<Option<EvalRun>> {
        let run_row = sqlx::query(
            "SELECT id, app_id, status, started_by, created_at, completed_at, \
                    total_questions, completed_questions, summary, error \
             FROM eval_runs WHERE id = $1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get eval run")?;

        let run_row = match run_row {
            Some(r) => r,
            None => return Ok(None),
        };

        let result_rows = sqlx::query(
            "SELECT question_id, question, expected_category, expected_key_points, \
                    actual_answer, actual_rationale, actual_category, actual_sources, \
                    chat_session_id, judge_score, judge_verdict, judge_reasoning, \
                    evaluated_at, error \
             FROM eval_results WHERE run_id = $1 ORDER BY evaluated_at",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get eval results")?;

        let results = result_rows
            .iter()
            .map(|r| EvalResult {
                question_id: r.get("question_id"),
                question: r.get("question"),
                expected_category: r.get("expected_category"),
                expected_key_points: r.get("expected_key_points"),
                actual_answer: r.get("actual_answer"),
                actual_rationale: r.get("actual_rationale"),
                actual_category: r.get("actual_category"),
                actual_sources: r.get("actual_sources"),
                chat_session_id: r.get("chat_session_id"),
                judge_score: r.get::<Option<i16>, _>("judge_score").map(|s| s as u8),
                judge_verdict: r.get("judge_verdict"),
                judge_reasoning: r.get("judge_reasoning"),
                evaluated_at: r.get("evaluated_at"),
                error: r.get("error"),
            })
            .collect();

        let summary: Option<EvalSummary> = run_row
            .get::<Option<serde_json::Value>, _>("summary")
            .and_then(|v| serde_json::from_value(v).ok());

        Ok(Some(EvalRun {
            id: run_row.get("id"),
            app_id: run_row.get("app_id"),
            status: EvalRunStatus::from_str(run_row.get("status")),
            started_by: run_row.get("started_by"),
            created_at: run_row.get("created_at"),
            completed_at: run_row.get("completed_at"),
            total_questions: run_row.get::<i32, _>("total_questions") as u32,
            completed_questions: run_row.get::<i32, _>("completed_questions") as u32,
            results,
            summary,
            error: run_row.get("error"),
        }))
    }

    pub async fn list_runs(&self, limit: i64) -> Result<Vec<EvalRun>> {
        let rows = sqlx::query(
            "SELECT id, app_id, status, started_by, created_at, completed_at, \
                    total_questions, completed_questions, summary, error \
             FROM eval_runs WHERE app_id = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(APP_ID)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to list eval runs")?;

        Ok(rows
            .iter()
            .map(|r| {
                let summary: Option<EvalSummary> = r
                    .get::<Option<serde_json::Value>, _>("summary")
                    .and_then(|v| serde_json::from_value(v).ok());
                EvalRun {
                    id: r.get("id"),
                    app_id: r.get("app_id"),
                    status: EvalRunStatus::from_str(r.get("status")),
                    started_by: r.get("started_by"),
                    created_at: r.get("created_at"),
                    completed_at: r.get("completed_at"),
                    total_questions: r.get::<i32, _>("total_questions") as u32,
                    completed_questions: r.get::<i32, _>("completed_questions") as u32,
                    results: vec![],
                    summary,
                    error: r.get("error"),
                }
            })
            .collect())
    }
}
