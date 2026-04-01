-- Add points column to test questions (default 1 point per question)
ALTER TABLE test_questions ADD COLUMN IF NOT EXISTS points SMALLINT NOT NULL DEFAULT 1;
