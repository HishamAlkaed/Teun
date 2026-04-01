import type { ChatEvent } from "./types";

/**
 * Parse SSE events from a streaming response.
 * Yields ChatEvent objects as they arrive.
 */
export async function* streamSSE(
  response: Response
): AsyncGenerator<ChatEvent> {
  const reader = response.body?.getReader();
  if (!reader) throw new Error("No response body");

  const decoder = new TextDecoder();
  let buffer = "";
  let currentEvent = "";

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split("\n");
    buffer = lines.pop() ?? "";

    for (const line of lines) {
      if (line.startsWith("event: ")) {
        currentEvent = line.slice(7).trim();
      } else if (line.startsWith("data: ")) {
        const data = line.slice(6);
        if (currentEvent && data) {
          try {
            const parsed = JSON.parse(data) as Record<string, unknown>;
            const eventData = (parsed.data ?? parsed) as ChatEvent["data"];
            yield { type: currentEvent, data: eventData } as ChatEvent;
          } catch {
            // Skip malformed JSON
          }
        }
        currentEvent = "";
      }
    }
  }
}
