import { useState } from "react";
import Markdown from "react-markdown";
import type { ChatMessage, MessageFeedback } from "../lib/types";
import type { ChatMode } from "../hooks/useSettings";

interface AssistantResponseProps {
  message: ChatMessage;
  chatMode?: ChatMode;
  onSelect?: () => void;
  onFeedback?: (messageId: string, feedback: MessageFeedback) => void;
  onAddToEval?: () => void;
}

export function AssistantResponse({ message, chatMode, onSelect, onFeedback, onAddToEval }: AssistantResponseProps) {
  const [copied, setCopied] = useState(false);
  const timeline = message.timeline ?? [];
  const isStreaming = message.isStreaming ?? false;
  const isInline = chatMode === "inline";
  const lastPartial = [...timeline].reverse().find((e) => e.type === "partial");
  const streamingContent = isInline && isStreaming && lastPartial?.data.content;
  const errorEntry = timeline.find((e) => e.type === "error");

  const answerText = message.structuredAnswer?.answer;
  // Only show the final answer in the main chat — thinking/partial text goes to the sidebar
  const displayText = answerText || (!isStreaming ? (streamingContent || message.content) : undefined);

  if (isStreaming && !displayText) {
    // Show a thinking indicator while the agent is working
    const lastThinking = [...timeline].reverse().find((e) => e.type === "thinking");
    const thinkingLabel = lastThinking?.data.content || "Aan het nadenken...";
    return (
      <div className="flex justify-start gap-2.5">
        <TeunAvatar />
        <div className="max-w-[80%] rounded-2xl rounded-bl-sm bg-[#EEF7F4] border border-dashed border-[#00B67A] px-4 py-3 text-[15px]">
          <div className="flex items-center gap-2">
            <svg className="h-4 w-4 timeline-spinner flex-shrink-0 text-[#00B67A]" viewBox="0 0 24 24" fill="none">
              <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="2" opacity="0.3" />
              <path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            </svg>
            <span className="text-text-secondary text-sm">{thinkingLabel}</span>
          </div>
        </div>
      </div>
    );
  }

  if (errorEntry) {
    return (
      <div className="flex justify-start gap-2.5">
        <TeunAvatar />
        <div className="max-w-[80%] rounded-2xl rounded-bl-sm bg-red-50 border border-red-100 px-4 py-3 text-sm text-red-700">
          {errorEntry.data.message}
        </div>
      </div>
    );
  }

  const showActions = message.structuredAnswer && !isStreaming;

  const handleCopy = async () => {
    if (!displayText) return;
    try {
      await navigator.clipboard.writeText(displayText);
    } catch {
      // Fallback for non-HTTPS contexts
      const ta = document.createElement("textarea");
      ta.value = displayText;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="flex justify-start gap-2.5">
      <TeunAvatar />
      <div className="max-w-[80%]">
        <div
          onClick={onSelect}
          className="rounded-2xl rounded-bl-sm bg-[#EEF7F4] border border-[#00B67A] text-text-primary px-4 py-3 text-[15px] leading-relaxed text-left cursor-pointer select-text hover:brightness-[0.98] prose prose-sm max-w-none prose-p:my-1.5 prose-ul:my-1.5 prose-ol:my-1.5 prose-li:my-0.5 prose-headings:my-2 prose-strong:text-text-primary"
        >
          <Markdown>{displayText}</Markdown>
          {isStreaming && (
            <span className="inline-block w-0.5 h-4 bg-[#00B67A]/60 ml-0.5 align-text-bottom timeline-dot-active" />
          )}
        </div>
        {showActions && (
          <div className="mt-1 flex items-center gap-0.5 pl-1">
            {/* Copy */}
            <ActionButton
              title={copied ? "Gekopieerd!" : "Kopieer"}
              onClick={handleCopy}
              active={copied}
            >
              {copied ? (
                <svg className="h-[18px] w-[18px]" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
                </svg>
              ) : (
                <svg className="h-[18px] w-[18px]" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 17.25v3.375c0 .621-.504 1.125-1.125 1.125h-9.75a1.125 1.125 0 0 1-1.125-1.125V7.875c0-.621.504-1.125 1.125-1.125H6.75a9.06 9.06 0 0 1 1.5.124m7.5 10.376h3.375c.621 0 1.125-.504 1.125-1.125V11.25c0-4.46-3.243-8.161-7.5-8.876a9.06 9.06 0 0 0-1.5-.124H9.375c-.621 0-1.125.504-1.125 1.125v3.5m7.5 10.375H9.375a1.125 1.125 0 0 1-1.125-1.125v-9.25m12 6.625v-1.875a3.375 3.375 0 0 0-3.375-3.375h-1.5a1.125 1.125 0 0 1-1.125-1.125v-1.5a3.375 3.375 0 0 0-3.375-3.375H9.75" />
                </svg>
              )}
            </ActionButton>

            {/* Thumbs up */}
            {onFeedback && (
              <>
                <ActionButton
                  title="Goedkeuren"
                  onClick={() => {
                    if (message.feedback?.status === "approved") {
                      onFeedback(message.id, { status: undefined as unknown as MessageFeedback["status"] });
                    } else {
                      onFeedback(message.id, { status: "approved" });
                    }
                  }}
                  active={message.feedback?.status === "approved"}
                >
                  <svg className="h-[18px] w-[18px]" fill={message.feedback?.status === "approved" ? "currentColor" : "none"} viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M6.633 10.25c.806 0 1.533-.446 2.031-1.08a9.041 9.041 0 0 1 2.861-2.4c.723-.384 1.35-.956 1.653-1.715a4.498 4.498 0 0 0 .322-1.672V3a.75.75 0 0 1 .75-.75 2.25 2.25 0 0 1 2.25 2.25c0 1.152-.26 2.243-.723 3.218-.266.558.107 1.282.725 1.282h3.126c1.026 0 1.945.694 2.054 1.715.045.422.068.85.068 1.285a11.95 11.95 0 0 1-2.649 7.521c-.388.482-.987.729-1.605.729H14.23c-.483 0-.964-.078-1.423-.23l-3.114-1.04a4.501 4.501 0 0 0-1.423-.23H5.904m.729-14.1a3 3 0 0 0-2.122-.879H2.75a.75.75 0 0 0-.75.75v14.25c0 .414.336.75.75.75h1.761a3 3 0 0 0 2.122-.879l.097-.097" />
                  </svg>
                </ActionButton>

                {/* Thumbs down */}
                <ActionButton
                  title="Afkeuren"
                  onClick={() => {
                    if (message.feedback?.status === "rejected") {
                      onFeedback(message.id, { status: undefined as unknown as MessageFeedback["status"] });
                    } else {
                      onFeedback(message.id, { status: "rejected" });
                    }
                  }}
                  active={message.feedback?.status === "rejected"}
                >
                  <svg className="h-[18px] w-[18px] rotate-180" fill={message.feedback?.status === "rejected" ? "currentColor" : "none"} viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M6.633 10.25c.806 0 1.533-.446 2.031-1.08a9.041 9.041 0 0 1 2.861-2.4c.723-.384 1.35-.956 1.653-1.715a4.498 4.498 0 0 0 .322-1.672V3a.75.75 0 0 1 .75-.75 2.25 2.25 0 0 1 2.25 2.25c0 1.152-.26 2.243-.723 3.218-.266.558.107 1.282.725 1.282h3.126c1.026 0 1.945.694 2.054 1.715.045.422.068.85.068 1.285a11.95 11.95 0 0 1-2.649 7.521c-.388.482-.987.729-1.605.729H14.23c-.483 0-.964-.078-1.423-.23l-3.114-1.04a4.501 4.501 0 0 0-1.423-.23H5.904m.729-14.1a3 3 0 0 0-2.122-.879H2.75a.75.75 0 0 0-.75.75v14.25c0 .414.336.75.75.75h1.761a3 3 0 0 0 2.122-.879l.097-.097" />
                  </svg>
                </ActionButton>
              </>
            )}

            {/* Add to eval */}
            {onAddToEval && (
              <ActionButton title="Toevoegen aan eval set" onClick={onAddToEval}>
                <svg className="h-[18px] w-[18px]" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 0 0-3.375-3.375h-1.5A1.125 1.125 0 0 1 13.5 7.125v-1.5a3.375 3.375 0 0 0-3.375-3.375H8.25m3.75 9v6m3-3H9m1.5-12H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 0 0-9-9Z" />
                </svg>
              </ActionButton>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function ActionButton({ title, onClick, active, children }: { title: string; onClick: () => void; active?: boolean; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      title={title}
      className={`p-2.5 rounded-lg transition-colors hover:bg-surface-hover ${
        active
          ? "text-accent"
          : "text-gray hover:text-text-primary"
      }`}
    >
      {children}
    </button>
  );
}

function TeunAvatar() {
  return (
    <div className="shrink-0 w-7 h-7 mt-0.5 rounded-full bg-accent flex items-center justify-center">
      <span className="text-white text-sm font-bold leading-none">T</span>
    </div>
  );
}
