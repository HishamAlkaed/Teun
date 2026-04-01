import { useCallback, useEffect, useRef, useState } from "react";
import { getSession, sendMessage, submitFeedback } from "../lib/api";
import { streamSSE } from "../lib/sse";
import type { ChatMessage, JudgeResult, MessageFeedback, MortgageAnswer, SourceVerdict, TimelineEntry } from "../lib/types";
import type { ChatMode, SearchDepth, UILanguage } from "./useSettings";

const uuid = (): string =>
  typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
        const r = (Math.random() * 16) | 0;
        return (c === "x" ? r : (r & 0x3) | 0x8).toString(16);
      });

export function useChat(chatMode: ChatMode = "tools", language: UILanguage = "nl", searchDepth: SearchDepth = "quick") {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [sessionId, setSessionId] = useState<string | undefined>();
  // Track in-flight request title so the sidebar pending entry survives navigation
  const [pendingTitle, setPendingTitle] = useState<string | undefined>();
  // The in-progress messages, updated by the streaming loop regardless of what's displayed
  const pendingMessagesRef = useRef<ChatMessage[] | null>(null);
  // Whether the user is currently viewing the in-progress chat
  const viewingPendingRef = useRef(false);

  const send = useCallback(
    async (text: string) => {
      if (!text.trim() || isLoading) return;

      const userMsg: ChatMessage = {
        id: uuid(),
        role: "user",
        content: text,
        timestamp: new Date(),
      };

      const assistantMsg: ChatMessage = {
        id: uuid(),
        role: "assistant",
        content: "",
        timestamp: new Date(),
        isStreaming: true,
        timeline: [],
      };

      const initialMessages: ChatMessage[] = [...messages, userMsg, assistantMsg];
      pendingMessagesRef.current = initialMessages;
      viewingPendingRef.current = true;
      setMessages(initialMessages);
      setIsLoading(true);
      setPendingTitle(text.slice(0, 80));

      try {
        const response = await sendMessage(text, sessionId, chatMode, language, searchDepth);

        if (!response.ok) {
          if (response.status === 401) {
            window.location.href = "/";
            return;
          }
          throw new Error(`HTTP ${response.status}`);
        }

        let accumulatedContent = "";
        let structuredAnswer: MortgageAnswer | undefined;
        let judgeResult: JudgeResult | undefined;
        const timeline: TimelineEntry[] = [];

        const updatePending = (updates: Partial<ChatMessage>) => {
          // Always update the ref (source of truth for pending state)
          if (pendingMessagesRef.current) {
            pendingMessagesRef.current = pendingMessagesRef.current.map((m) =>
              m.id === assistantMsg.id ? { ...m, ...updates } : m
            );
          }
          // Only update displayed messages if user is viewing the pending chat
          if (viewingPendingRef.current && pendingMessagesRef.current) {
            setMessages(pendingMessagesRef.current);
          }
        };

        for await (const event of streamSSE(response)) {
          const entry: TimelineEntry = {
            id: uuid(),
            type: event.type,
            timestamp: new Date(),
            data: {},
          };

          switch (event.type) {
            case "thinking":
              entry.data.content = event.data.content;
              break;

            case "tool_use":
              entry.data.tool = event.data.tool;
              entry.data.toolLabel = event.data.tool
                .replace(/^mcp__[^_]+__/, "")
                .replace(/_/g, " ");
              entry.data.input = event.data.input;
              entry.data.content = event.data.content;
              break;

            case "partial":
              accumulatedContent = event.data.content;
              entry.data.content = event.data.content;
              break;

            case "result": {
              const so = event.data.structured_output as unknown as Record<string, unknown> | undefined;
              if (so && typeof so === "object" && "answer" in so) {
                structuredAnswer = {
                  answer: typeof so.answer === "string" ? so.answer : "",
                  rationale: typeof so.rationale === "string" ? so.rationale : "",
                  sources: Array.isArray(so.sources) ? so.sources as MortgageAnswer["sources"] : [],
                  category: so.category === "doorverwijzen_speciale_afhandeling"
                    ? "doorverwijzen_speciale_afhandeling" : "standard",
                };
              }
              entry.data.structured_output = structuredAnswer;
              if (event.data.session_id) {
                setSessionId(event.data.session_id);
                setPendingTitle(undefined);
              }
              break;
            }

            case "judge": {
              // Backend serializes as { result: { score, ... } } due to serde tagged enum
              const raw = event.data as unknown as Record<string, unknown>;
              const jd = (raw.result ?? raw) as Record<string, unknown>;
              judgeResult = {
                source_verdicts: Array.isArray(jd.source_verdicts) ? jd.source_verdicts as SourceVerdict[] : [],
                sources_verified: Number(jd.sources_verified ?? 0),
                sources_total: Number(jd.sources_total ?? 0),
                score: typeof jd.score === "number" ? jd.score : undefined,
                reasoning: typeof jd.reasoning === "string" ? jd.reasoning : undefined,
                llm_skipped: Boolean(jd.llm_skipped),
                llm_error: typeof jd.llm_error === "string" ? jd.llm_error : undefined,
              };
              entry.data.judge_result = judgeResult;
              break;
            }

            case "error":
              entry.data.message = event.data.message;
              break;
          }

          timeline.push(entry);
          updatePending({ content: accumulatedContent, structuredAnswer, judgeResult, timeline: [...timeline] });
        }

        updatePending({
          content: structuredAnswer?.answer ?? accumulatedContent,
          structuredAnswer,
          judgeResult,
          isStreaming: false,
          timeline: [...timeline],
        });
      } catch (err) {
        const errorDetail = err instanceof Error ? err.message : "onbekend";
        const errorContent = `Teun heeft op dit moment een storing. Probeer het later nog eens. Error: ${errorDetail}`;
        if (pendingMessagesRef.current) {
          pendingMessagesRef.current = pendingMessagesRef.current.map((m) =>
            m.id === assistantMsg.id ? { ...m, content: errorContent, isStreaming: false } : m
          );
        }
        if (viewingPendingRef.current && pendingMessagesRef.current) {
          setMessages(pendingMessagesRef.current);
        }
      } finally {
        setIsLoading(false);
        setPendingTitle(undefined);
        pendingMessagesRef.current = null;
        viewingPendingRef.current = false;
      }
    },
    [isLoading, messages, sessionId, chatMode, language, searchDepth]
  );

  // Navigate back to the in-progress chat
  const returnToPending = useCallback(() => {
    if (pendingMessagesRef.current) {
      viewingPendingRef.current = true;
      setMessages(pendingMessagesRef.current);
      setSessionId(undefined);
    }
  }, []);

  // Reset session when chat mode changes
  const prevMode = useRef(chatMode);
  useEffect(() => {
    if (prevMode.current !== chatMode) {
      prevMode.current = chatMode;
      setMessages([]);
      setSessionId(undefined);
    }
  }, [chatMode]);

  const loadSession = useCallback(async (id: string) => {
    viewingPendingRef.current = false;
    try {
      const session = await getSession(id);
      const loaded: ChatMessage[] = session.messages.map((m) => {
        let sa: MortgageAnswer | undefined;
        const raw = m.structured_answer as Record<string, unknown> | undefined;
        if (raw && typeof raw === "object" && "answer" in raw) {
          sa = {
            answer: typeof raw.answer === "string" ? raw.answer : "",
            rationale: typeof raw.rationale === "string" ? raw.rationale : "",
            sources: Array.isArray(raw.sources) ? raw.sources as MortgageAnswer["sources"] : [],
            category: raw.category === "doorverwijzen_speciale_afhandeling"
              ? "doorverwijzen_speciale_afhandeling" : "standard",
          };
        }

        let jr: JudgeResult | undefined;
        const jraw = m.judge_result as Record<string, unknown> | undefined;
        if (jraw && typeof jraw === "object") {
          jr = {
            source_verdicts: Array.isArray(jraw.source_verdicts) ? jraw.source_verdicts as SourceVerdict[] : [],
            sources_verified: Number(jraw.sources_verified ?? 0),
            sources_total: Number(jraw.sources_total ?? 0),
            score: typeof jraw.score === "number" ? jraw.score : undefined,
            reasoning: typeof jraw.reasoning === "string" ? jraw.reasoning : undefined,
            llm_skipped: Boolean(jraw.llm_skipped),
            llm_error: typeof jraw.llm_error === "string" ? jraw.llm_error : undefined,
          };
        }

        return {
          id: m.id,
          role: m.role as "user" | "assistant",
          content: m.content,
          structuredAnswer: sa,
          judgeResult: jr,
          timestamp: new Date(m.timestamp),
          timeline: Array.isArray(m.timeline) ? (m.timeline as TimelineEntry[]) : undefined,
          feedback: m.feedback,
        };
      });
      setMessages(loaded);
      setSessionId(id);
    } catch (err) {
      console.error("Failed to load session:", err);
    }
  }, []);

  const updateMessageFeedback = useCallback(
    async (messageId: string, feedback: MessageFeedback) => {
      if (!sessionId) return;
      // If status is falsy, clear feedback locally (toggle off)
      if (!feedback.status) {
        setMessages((prev) =>
          prev.map((m) => (m.id === messageId ? { ...m, feedback: undefined } : m)),
        );
        return;
      }
      try {
        await submitFeedback(sessionId, messageId, feedback.status, feedback.comment);
        setMessages((prev) =>
          prev.map((m) => (m.id === messageId ? { ...m, feedback } : m)),
        );
      } catch (err) {
        console.error("Failed to submit feedback:", err);
      }
    },
    [sessionId],
  );

  const newSession = useCallback(() => {
    viewingPendingRef.current = false;
    setMessages([]);
    setSessionId(undefined);
  }, []);

  const addSystemMessage = useCallback((content: string) => {
    setMessages((prev) => [...prev, {
      id: uuid(),
      role: "system" as const,
      content,
      timestamp: new Date(),
    }]);
  }, []);

  return { messages, send, isLoading, sessionId, pendingTitle, returnToPending, loadSession, newSession, updateMessageFeedback, addSystemMessage };
}
