import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  sendMessage,
  listSessions,
  deleteSession,
  submitFeedback,
  scrubMessage,
  getBackendSettings,
  updateBackendSettings,
} from "./api";

const mockFetch = vi.fn();
vi.stubGlobal("fetch", mockFetch);

function jsonResponse(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

beforeEach(() => {
  mockFetch.mockReset();
});

describe("sendMessage", () => {
  it("sends POST with correct body", async () => {
    mockFetch.mockResolvedValue(new Response("ok"));
    await sendMessage("hello", "sess-1", "tools", "nl", "quick");
    expect(mockFetch).toHaveBeenCalledWith("/api/teun/chat", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        message: "hello",
        session_id: "sess-1",
        mode: "tools",
        language: "nl",
        search_depth: "quick",
      }),
    });
  });

  it("uses defaults for optional params", async () => {
    mockFetch.mockResolvedValue(new Response("ok"));
    await sendMessage("hello");
    const body = JSON.parse(mockFetch.mock.calls[0][1].body);
    expect(body.mode).toBe("tools");
    expect(body.language).toBe("nl");
    expect(body.search_depth).toBe("quick");
  });
});

describe("listSessions", () => {
  it("returns parsed sessions", async () => {
    const sessions = [{ id: "1", title: "Test" }];
    mockFetch.mockResolvedValue(jsonResponse(sessions));
    const result = await listSessions();
    expect(result).toEqual(sessions);
  });

  it("throws on non-ok response", async () => {
    mockFetch.mockResolvedValue(new Response("", { status: 500 }));
    await expect(listSessions()).rejects.toThrow("HTTP 500");
  });
});

describe("deleteSession", () => {
  it("sends DELETE request", async () => {
    mockFetch.mockResolvedValue(new Response("", { status: 200 }));
    await deleteSession("abc");
    expect(mockFetch).toHaveBeenCalledWith(
      "/api/teun/sessions/abc",
      expect.objectContaining({ method: "DELETE" }),
    );
  });
});

describe("submitFeedback", () => {
  it("sends PUT with status and comment", async () => {
    mockFetch.mockResolvedValue(new Response("", { status: 200 }));
    await submitFeedback("sess-1", "msg-1", "approved", "Looks good");
    const call = mockFetch.mock.calls[0];
    expect(call[0]).toBe(
      "/api/teun/sessions/sess-1/messages/msg-1/feedback",
    );
    expect(call[1].method).toBe("PUT");
    const body = JSON.parse(call[1].body);
    expect(body.status).toBe("approved");
    expect(body.comment).toBe("Looks good");
  });
});

describe("scrubMessage", () => {
  it("returns scrub response", async () => {
    const scrubResp = {
      scrubbed_text: "Mijn [BSN_1] is geheim",
      entities: [{ entity_type: "BSN", original: "123456789", placeholder: "[BSN_1]" }],
    };
    mockFetch.mockResolvedValue(jsonResponse(scrubResp));
    const result = await scrubMessage("Mijn 123456789 is geheim");
    expect(result.scrubbed_text).toBe("Mijn [BSN_1] is geheim");
    expect(result.entities).toHaveLength(1);
  });

  it("throws on failure", async () => {
    mockFetch.mockResolvedValue(new Response("", { status: 503 }));
    await expect(scrubMessage("test")).rejects.toThrow("Scrub failed");
  });
});

describe("getBackendSettings", () => {
  it("returns settings", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ scrub_enabled: true }));
    const result = await getBackendSettings();
    expect(result.scrub_enabled).toBe(true);
  });
});

describe("updateBackendSettings", () => {
  it("sends PUT with partial settings", async () => {
    mockFetch.mockResolvedValue(jsonResponse({ scrub_enabled: false }));
    await updateBackendSettings({ scrub_enabled: false });
    const call = mockFetch.mock.calls[0];
    expect(call[1].method).toBe("PUT");
    expect(JSON.parse(call[1].body)).toEqual({ scrub_enabled: false });
  });
});
