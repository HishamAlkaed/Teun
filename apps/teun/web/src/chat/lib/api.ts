import type { BackendSettings, MessageFeedback, ScrubResponse, StoredSession } from "./types";

const API_BASE = "/api/teun";

export async function sendMessage(
  message: string,
  sessionId?: string,
  mode?: string,
  language?: string,
  searchDepth?: string,
  signal?: AbortSignal,
): Promise<Response> {
  return fetch(`${API_BASE}/chat`, {
    method: "POST",
    signal,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      message,
      session_id: sessionId,
      mode: mode ?? "tools",
      language: language ?? "nl",
      search_depth: searchDepth ?? "quick",
    }),
  });
}

export async function listSessions(): Promise<StoredSession[]> {
  const res = await fetch(`${API_BASE}/sessions`, {

  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return await res.json() as StoredSession[];
}

export async function getSession(id: string): Promise<{
  id: string;
  title: string;
  messages: Array<{
    id: string;
    role: string;
    content: string;
    structured_answer?: unknown;
    timestamp: string;
    timeline?: unknown;
    feedback?: MessageFeedback;
    judge_result?: unknown;
  }>;
}> {
  const res = await fetch(`${API_BASE}/sessions/${id}`, {

  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return await res.json() as {
    id: string;
    title: string;
    messages: Array<{
      id: string;
      role: string;
      content: string;
      structured_answer?: unknown;
      timestamp: string;
      timeline?: unknown;
      feedback?: MessageFeedback;
      judge_result?: unknown;
    }>;
  };
}

export async function deleteSession(id: string): Promise<void> {
  const res = await fetch(`${API_BASE}/sessions/${id}`, {
    method: "DELETE",

  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
}

export async function submitFeedback(
  sessionId: string,
  messageId: string,
  status: string,
  comment?: string,
): Promise<void> {
  const res = await fetch(
    `${API_BASE}/sessions/${sessionId}/messages/${messageId}/feedback`,
    {
      method: "PUT",
  
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ status, comment }),
    },
  );
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
}

export async function scrubMessage(text: string): Promise<ScrubResponse> {
  const res = await fetch(`${API_BASE}/scrub`, {
    method: "POST",

    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text }),
  });
  if (!res.ok) throw new Error(`Scrub failed: HTTP ${res.status}`);
  return await res.json() as ScrubResponse;
}

export async function getBackendSettings(): Promise<BackendSettings> {
  const res = await fetch(`${API_BASE}/settings`, {

  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return await res.json() as BackendSettings;
}

export async function updateBackendSettings(
  settings: Partial<BackendSettings>,
): Promise<BackendSettings> {
  const res = await fetch(`${API_BASE}/settings`, {
    method: "PUT",

    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(settings),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return await res.json() as BackendSettings;
}
