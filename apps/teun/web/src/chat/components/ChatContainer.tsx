import { useEffect, useRef } from "react";
import type { ChatMessage, MessageFeedback } from "../lib/types";
import type { ChatMode } from "../hooks/useSettings";
import { ChatInput } from "./ChatInput";
import { UserMessage } from "./UserMessage";
import { AssistantResponse } from "./AssistantResponse";

interface ChatContainerProps {
  messages: ChatMessage[];
  send: (text: string) => void;
  isLoading: boolean;
  chatMode?: ChatMode;
  scrubEnabled?: boolean;
  onScrubSent?: () => void;
  selectedMessageId?: string;
  onSelectMessage?: (id: string) => void;
  onFeedback?: (messageId: string, feedback: MessageFeedback) => void;
  onAddToEval?: (messageId: string) => void;
}

export function ChatContainer({ messages, send, isLoading, chatMode, scrubEnabled, onScrubSent, onSelectMessage, onFeedback, onAddToEval }: ChatContainerProps) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-y-auto py-6 px-4 space-y-4">
        {messages.length === 0 && (
          <div className="flex flex-col items-center justify-center py-20 text-center">
            <svg className="w-16 h-16 mb-4 text-accent/20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
            </svg>
            <p className="text-sm text-text-tertiary">Stel je eerste vraag aan Teun</p>
          </div>
        )}
        {messages.map((msg) => {
          if (msg.role === "system") {
            return (
              <div key={msg.id} className="flex justify-center">
                <span className="inline-flex items-center gap-1.5 rounded-full border border-border bg-surface px-3 py-1 text-xs text-text-secondary">
                  {msg.content}
                </span>
              </div>
            );
          }
          if (msg.role === "user") {
            return <UserMessage key={msg.id} message={msg} />;
          }
          return (
            <AssistantResponse
              key={msg.id}
              message={msg}
              chatMode={chatMode}
              onSelect={() => onSelectMessage?.(msg.id)}
              onFeedback={onFeedback}
              onAddToEval={onAddToEval ? () => onAddToEval(msg.id) : undefined}
            />
          );
        })}
        <div ref={bottomRef} />
      </div>
      <div className="border-t border-border bg-page-bg px-4 py-3">
        <ChatInput onSend={send} disabled={isLoading} scrubEnabled={scrubEnabled} onScrubSent={onScrubSent} />
      </div>
    </div>
  );
}
