import { describe, it, expect } from "vitest";
import { streamSSE } from "./sse";

function makeResponse(chunks: string[]): Response {
  const encoder = new TextEncoder();
  let index = 0;
  const stream = new ReadableStream<Uint8Array>({
    pull(controller) {
      if (index < chunks.length) {
        controller.enqueue(encoder.encode(chunks[index]));
        index++;
      } else {
        controller.close();
      }
    },
  });
  return new Response(stream);
}

async function collectEvents(response: Response) {
  const events = [];
  for await (const event of streamSSE(response)) {
    events.push(event);
  }
  return events;
}

describe("streamSSE", () => {
  it("parses a single complete event", async () => {
    const response = makeResponse([
      'event: thinking\ndata: {"content":"Analyzing..."}\n\n',
    ]);
    const events = await collectEvents(response);
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("thinking");
    expect(events[0].data).toEqual({ content: "Analyzing..." });
  });

  it("parses multiple events in one chunk", async () => {
    const response = makeResponse([
      'event: thinking\ndata: {"content":"A"}\n\nevent: partial\ndata: {"content":"B"}\n\n',
    ]);
    const events = await collectEvents(response);
    expect(events).toHaveLength(2);
    expect(events[0].type).toBe("thinking");
    expect(events[1].type).toBe("partial");
  });

  it("handles events split across chunks", async () => {
    const response = makeResponse([
      "event: thinking\n",
      'data: {"content":"hello"}\n\n',
    ]);
    const events = await collectEvents(response);
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("thinking");
  });

  it("skips malformed JSON", async () => {
    const response = makeResponse([
      'event: thinking\ndata: {bad json}\n\nevent: partial\ndata: {"content":"ok"}\n\n',
    ]);
    const events = await collectEvents(response);
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("partial");
  });

  it("handles empty stream", async () => {
    const response = makeResponse([]);
    const events = await collectEvents(response);
    expect(events).toHaveLength(0);
  });

  it("throws when response has no body", async () => {
    const response = new Response(null);
    await expect(collectEvents(response)).rejects.toThrow("No response body");
  });

  it("handles nested data field", async () => {
    const response = makeResponse([
      'event: result\ndata: {"data":{"structured_output":{"answer":"test"},"session_id":"123"}}\n\n',
    ]);
    const events = await collectEvents(response);
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("result");
    expect(events[0].data).toEqual({
      structured_output: { answer: "test" },
      session_id: "123",
    });
  });

  it("ignores lines without event or data prefix", async () => {
    const response = makeResponse([
      'event: thinking\n: comment line\nid: 123\ndata: {"content":"x"}\n\n',
    ]);
    const events = await collectEvents(response);
    expect(events).toHaveLength(1);
  });
});
