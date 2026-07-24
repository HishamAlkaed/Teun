const API_BASE = "/api/teun";

// --- Test question types ---

export interface TestQuestion {
  id: string;
  question: string;
  expected_category: string | null;
  expected_key_points: string[];
  description: string | null;
  points: number;
  created_at: string;
  updated_at: string;
}

export interface QuestionPayload {
  question: string;
  expected_category: string | null;
  expected_key_points: string[];
  description: string | null;
  points: number;
}

// --- Eval types ---

export interface EvalRunSummary {
  id: string;
  status: "pending" | "running" | "completed" | "failed" | "stopped";
  started_by: string;
  created_at: string;
  completed_at: string | null;
  total_questions: number;
  completed_questions: number;
  summary: EvalSummary | null;
}

export interface EvalSummary {
  pass_count: number;
  partial_count: number;
  fail_count: number;
  error_count: number;
  average_score: number;
  pass_rate: number;
}

export interface EvalResult {
  question_id: string;
  question: string;
  expected_category: string | null;
  expected_key_points: string[];
  actual_answer: string | null;
  actual_rationale: string | null;
  actual_category: string | null;
  actual_sources: unknown;
  chat_session_id: string | null;
  judge_score: number | null;
  judge_verdict: "pass" | "partial" | "fail" | null;
  judge_reasoning: string | null;
  evaluated_at: string;
  error: string | null;
}

export interface EvalRun extends EvalRunSummary {
  results: EvalResult[];
  error: string | null;
}

// --- Feedback types ---

export interface FeedbackStats {
  total: number;
  approved: number;
  partial: number;
  rejected: number;
  approval_rate: number;
  partial_rate: number;
  rejection_rate: number;
  trend: TrendEntry[];
}

export interface TrendEntry {
  date: string;
  status: string;
  count: number;
}

export interface RejectedMessage {
  session_id: string;
  session_title: string;
  message_id: string;
  answer: string;
  structured_answer: unknown;
  feedback_status: string;
  feedback_comment: string | null;
  feedback_at: string;
  timestamp: string;
}

// --- Questions ---

export async function listQuestions(): Promise<TestQuestion[]> {
  const res = await fetch(`${API_BASE}/questions`);
  if (!res.ok) return [];
  return await res.json() as TestQuestion[];
}

export async function createQuestion(payload: QuestionPayload): Promise<TestQuestion> {
  const res = await fetch(`${API_BASE}/questions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({})) as { error?: string };
    throw new Error(data.error || "Kon vraag niet aanmaken");
  }
  return await res.json() as TestQuestion;
}

export async function updateQuestion(id: string, payload: QuestionPayload): Promise<void> {
  const res = await fetch(`${API_BASE}/questions/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({})) as { error?: string };
    throw new Error(data.error || "Kon vraag niet bijwerken");
  }
}

export async function deleteQuestion(id: string): Promise<void> {
  const res = await fetch(`${API_BASE}/questions/${id}`, {
    method: "DELETE",
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({})) as { error?: string };
    throw new Error(data.error || "Kon vraag niet verwijderen");
  }
}

// --- Eval ---

export async function startEval(mode?: string): Promise<{ id: string; status: string }> {
  const res = await fetch(`${API_BASE}/eval/runs`, {
    method: "POST",
    headers: mode ? { "Content-Type": "application/json" } : undefined,
    body: mode ? JSON.stringify({ mode }) : undefined,
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({})) as { error?: string };
    throw new Error(data.error || "Kon evaluatie niet starten");
  }
  return await res.json() as { id: string; status: string };
}

export async function listEvalRuns(): Promise<EvalRunSummary[]> {
  const res = await fetch(`${API_BASE}/eval/runs`);
  if (!res.ok) return [];
  return await res.json() as EvalRunSummary[];
}

export async function getEvalRun(id: string): Promise<EvalRun> {
  const res = await fetch(`${API_BASE}/eval/runs/${id}`);
  if (!res.ok) throw new Error("Kon evaluatie niet ophalen");
  return await res.json() as EvalRun;
}

export async function stopEval(id: string): Promise<void> {
  const res = await fetch(`${API_BASE}/eval/runs/${id}/stop`, {
    method: "POST",
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({})) as { error?: string };
    throw new Error(data.error || "Kon evaluatie niet stoppen");
  }
}

// --- Feedback ---

export async function getFeedbackStats(): Promise<FeedbackStats> {
  const res = await fetch(`${API_BASE}/feedback/stats`);
  if (!res.ok) throw new Error("Kon feedback niet ophalen");
  return await res.json() as FeedbackStats;
}

export async function getRecentRejections(): Promise<RejectedMessage[]> {
  const res = await fetch(`${API_BASE}/feedback/recent`);
  if (!res.ok) return [];
  return await res.json() as RejectedMessage[];
}

// --- Admin documents ---

export interface AdminDocument {
  id: string;
  filename: string;
  status: "pending" | "indexing" | "indexed" | "error";
  chunk_count: number;
  page_count: number;
  size_bytes: number;
  error_message: string | null;
  created_at: string;
  indexed_at: string | null;
}

export interface UploadResult {
  filename: string;
  status: string;
}

export async function listAdminDocuments(): Promise<AdminDocument[]> {
  const res = await fetch(`${API_BASE}/admin/documents`);
  if (!res.ok) return [];
  return await res.json() as AdminDocument[];
}

export async function uploadDocuments(files: FileList | File[]): Promise<UploadResult[]> {
  const formData = new FormData();
  for (const file of Array.from(files)) {
    formData.append("file", file);
  }
  const res = await fetch(`${API_BASE}/admin/documents`, {
    method: "POST",
    body: formData,
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({})) as { error?: string };
    throw new Error(data.error || "Upload mislukt");
  }
  return await res.json() as UploadResult[];
}

export async function deleteDocument(id: string): Promise<void> {
  const res = await fetch(`${API_BASE}/admin/documents/${id}`, {
    method: "DELETE",
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({})) as { error?: string };
    throw new Error(data.error || "Kon document niet verwijderen");
  }
}
