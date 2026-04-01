-- Sessions
CREATE TABLE IF NOT EXISTS sessions (
    id          VARCHAR(36) PRIMARY KEY,
    title       VARCHAR(255) NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_active TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_sessions_last_active ON sessions (last_active DESC);

-- Messages (normalized from embedded array)
CREATE TABLE IF NOT EXISTS messages (
    id                  VARCHAR(36) PRIMARY KEY,
    session_id          VARCHAR(36) NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role                VARCHAR(20) NOT NULL,
    content             TEXT NOT NULL DEFAULT '',
    structured_answer   JSONB,
    timestamp           TIMESTAMPTZ NOT NULL DEFAULT now(),
    timeline            JSONB,
    judge_result        JSONB,
    feedback_status     VARCHAR(20),
    feedback_comment    TEXT,
    feedback_created_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_messages_session_id ON messages (session_id);
CREATE INDEX IF NOT EXISTS idx_messages_feedback ON messages (feedback_status) WHERE feedback_status IS NOT NULL;

-- Test questions
CREATE TABLE IF NOT EXISTS test_questions (
    id                  VARCHAR(36) PRIMARY KEY,
    app_id              VARCHAR(50) NOT NULL DEFAULT 'teun',
    question            TEXT NOT NULL,
    expected_category   VARCHAR(255),
    expected_key_points TEXT[] NOT NULL DEFAULT '{}',
    description         TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_test_questions_app_id ON test_questions (app_id);

-- Eval runs
CREATE TABLE IF NOT EXISTS eval_runs (
    id                  VARCHAR(36) PRIMARY KEY,
    app_id              VARCHAR(50) NOT NULL DEFAULT 'teun',
    status              VARCHAR(20) NOT NULL DEFAULT 'pending',
    started_by          VARCHAR(255) NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at        TIMESTAMPTZ,
    total_questions     INTEGER NOT NULL DEFAULT 0,
    completed_questions INTEGER NOT NULL DEFAULT 0,
    summary             JSONB,
    error               TEXT
);

CREATE INDEX IF NOT EXISTS idx_eval_runs_app_id_created ON eval_runs (app_id, created_at DESC);

-- Eval results (normalized from embedded array)
CREATE TABLE IF NOT EXISTS eval_results (
    id                  VARCHAR(36) PRIMARY KEY DEFAULT gen_random_uuid()::text,
    run_id              VARCHAR(36) NOT NULL REFERENCES eval_runs(id) ON DELETE CASCADE,
    question_id         VARCHAR(36) NOT NULL,
    question            TEXT NOT NULL,
    expected_category   VARCHAR(255),
    expected_key_points TEXT[] NOT NULL DEFAULT '{}',
    actual_answer       TEXT,
    actual_rationale    TEXT,
    actual_category     VARCHAR(255),
    actual_sources      JSONB,
    chat_session_id     VARCHAR(36),
    judge_score         SMALLINT,
    judge_verdict       VARCHAR(20),
    judge_reasoning     TEXT,
    evaluated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    error               TEXT
);

CREATE INDEX IF NOT EXISTS idx_eval_results_run_id ON eval_results (run_id);

-- Settings (key-value with JSONB)
CREATE TABLE IF NOT EXISTS settings (
    key   VARCHAR(100) PRIMARY KEY,
    value JSONB NOT NULL DEFAULT '{}'
);
