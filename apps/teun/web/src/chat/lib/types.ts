export interface SourceReference {
  document: string;
  section: string;
  quote?: string;
  line_range?: string;
}

export interface MortgageAnswer {
  answer: string;
  rationale: string;
  sources: SourceReference[];
  category: "standard" | "mandaat_uitzondering" | "doorverwijzen_speciale_afhandeling";
}

export type SourceVerdictStatus =
  | "ok"
  | "document_not_found"
  | "line_range_out_of_bounds"
  | "quote_mismatch";

export interface SourceVerdict {
  document: string;
  section: string;
  line_range?: string;
  status: SourceVerdictStatus;
  detail?: string;
}

export interface JudgeResult {
  source_verdicts: SourceVerdict[];
  sources_verified: number;
  sources_total: number;
  score?: number;
  reasoning?: string;
  llm_skipped: boolean;
  llm_error?: string;
}

export interface TimelineEntry {
  id: string;
  type: "thinking" | "tool_use" | "partial" | "result" | "error" | "judge";
  timestamp: Date;
  data: {
    content?: string;
    tool?: string;
    toolLabel?: string;
    input?: unknown;
    structured_output?: MortgageAnswer;
    message?: string;
    judge_result?: JudgeResult;
  };
}

export interface MessageFeedback {
  status: "approved" | "partial" | "rejected";
  comment?: string;
  created_at?: string;
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  structuredAnswer?: MortgageAnswer;
  judgeResult?: JudgeResult;
  timestamp: Date;
  isStreaming?: boolean;
  timeline?: TimelineEntry[];
  feedback?: MessageFeedback;
}

export interface StoredSession {
  id: string;
  title: string;
  created_at: string;
  last_active: string;
}

export interface DetectedEntity {
  entity_type: string;
  original: string;
  placeholder: string;
}

export interface ScrubResponse {
  scrubbed_text: string;
  entities: DetectedEntity[];
}

export interface BackendSettings {
  scrub_enabled: boolean;
}

export type ChatEventType = "thinking" | "tool_use" | "partial" | "result" | "error" | "judge";

export interface ChatEventThinking {
  type: "thinking";
  data: { content: string };
}

export interface ChatEventToolUse {
  type: "tool_use";
  data: { tool: string; input: unknown; content?: string };
}

export interface ChatEventPartial {
  type: "partial";
  data: { content: string };
}

export interface ChatEventResult {
  type: "result";
  data: { structured_output: MortgageAnswer; session_id: string; message_id?: string };
}

export interface ChatEventError {
  type: "error";
  data: { message: string };
}

export interface ChatEventJudge {
  type: "judge";
  data: JudgeResult;
}

export type ChatEvent =
  | ChatEventThinking
  | ChatEventToolUse
  | ChatEventPartial
  | ChatEventResult
  | ChatEventError
  | ChatEventJudge;
