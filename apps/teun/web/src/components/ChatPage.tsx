import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";
import { useChat } from "../chat/hooks/useChat";
import { useSettings } from "../chat/hooks/useSettings";
import { ChatContainer } from "../chat/components/ChatContainer";
import { ReasoningSidebar } from "../chat/components/ReasoningSidebar";
import { SessionModal } from "../chat/components/SessionModal";
import { AddToEvalModal } from "../chat/components/AddToEvalModal";
import { getBackendSettings } from "../chat/lib/api";

export function ChatPage() {
  const { settings } = useSettings();
  const chat = useChat("inline", settings.language, "quick");
  const [scrubEnabled, setScrubEnabled] = useState(false);
  const [selectedMessageId, setSelectedMessageId] = useState<string | undefined>();
  const [sessionModalOpen, setSessionModalOpen] = useState(false);
  const [evalModalMessageId, setEvalModalMessageId] = useState<string | undefined>();
  const location = useLocation();
  const navigate = useNavigate();
  const { sessionId: urlSessionId } = useParams<{ sessionId?: string }>();
  const initialMessageSent = useRef(false);
  const urlSessionLoaded = useRef<string | undefined>(undefined);
  // Prevents Effect 2 from correcting the URL back to the old session while loadSession is in-flight
  const isLoadingSessionRef = useRef(false);

  useEffect(() => {
    getBackendSettings()
      .then((s) => setScrubEnabled(s.scrub_enabled))
      .catch(() => setScrubEnabled(false));
  }, []);

  // Load session from URL on mount or when URL changes
  useEffect(() => {
    if (urlSessionId && urlSessionId !== urlSessionLoaded.current && urlSessionId !== chat.sessionId) {
      urlSessionLoaded.current = urlSessionId;
      isLoadingSessionRef.current = true;
      void chat.loadSession(urlSessionId).finally(() => {
        isLoadingSessionRef.current = false;
      });
    }
  }, [urlSessionId, chat]);

  // Sync URL when sessionId changes (after sending a message or loading a session).
  // Skip while a session load is in-flight to avoid bouncing the URL back to the old session.
  useEffect(() => {
    if (isLoadingSessionRef.current) return;
    if (chat.sessionId && chat.sessionId !== urlSessionId) {
      navigate(`/chat/${chat.sessionId}`, { replace: true });
    } else if (!chat.sessionId && urlSessionId) {
      navigate("/chat", { replace: true });
    }
  }, [chat.sessionId, urlSessionId, navigate]);

  useEffect(() => {
    const state = location.state as { initialMessage?: string } | null;
    if (state?.initialMessage && !initialMessageSent.current) {
      initialMessageSent.current = true;
      chat.send(state.initialMessage);
      navigate(location.pathname, { replace: true, state: {} });
    }
  }, [location.state, chat, navigate, location.pathname]);

  const handleScrubSent = useCallback(() => {
    chat.addSystemMessage("PII is gedetecteerd en geanonimiseerd in dit bericht.");
  }, [chat]);

  const handleNewSession = useCallback(() => {
    chat.newSession();
    navigate("/chat", { replace: true });
  }, [chat, navigate]);

  const handleSelectSession = useCallback((id: string) => {
    navigate(`/chat/${id}`, { replace: true });
  }, [navigate]);

  // Auto-select the latest assistant message (or streaming one)
  const assistantMessages = chat.messages.filter((m) => m.role === "assistant");
  const streamingMsg = assistantMessages.find((m) => m.isStreaming);
  const latestAssistant = assistantMessages[assistantMessages.length - 1];
  const autoSelectedId = streamingMsg?.id ?? latestAssistant?.id;
  const activeMessageId = selectedMessageId ?? autoSelectedId;

  const selectedMessage = chat.messages.find((m) => m.id === activeMessageId) ?? null;
  const selectedUserQuestion = selectedMessage
    ? chat.messages.slice(0, chat.messages.indexOf(selectedMessage)).reverse().find((m) => m.role === "user")?.content
    : undefined;

  const handleFeedback = useCallback(
    (messageId: string, feedback: import("../chat/lib/types").MessageFeedback) => {
      chat.updateMessageFeedback(messageId, feedback);
    },
    [chat],
  );

  return (
    <div className="h-screen bg-page-bg flex flex-col overflow-hidden">
      {/* Header */}
      <header className="flex-shrink-0 px-4 py-3 md:px-6 md:py-4 flex items-center justify-between">
        <Link to="/" className="flex items-center gap-3 hover:opacity-80 transition-opacity">
          <TeunAvatar size={34} />
          <div>
            <div className="font-bold text-[15px] text-text-primary">Teun</div>
            <div className="text-xs text-accent-dark flex items-center gap-[5px]">
              <span className="w-1.5 h-1.5 rounded-full bg-accent" />
              Altijd beschikbaar voor jou
            </div>
          </div>
        </Link>
        <div className="flex items-center gap-1">
          {/* New conversation */}
          <button
            onClick={handleNewSession}
            title="Nieuw gesprek"
            className="p-2 rounded-lg text-text-tertiary hover:text-text-primary hover:bg-surface transition-colors"
          >
            <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
            </svg>
          </button>

          {/* Conversations */}
          <button
            onClick={() => setSessionModalOpen(true)}
            title="Gesprekken"
            className="p-2 rounded-lg text-text-tertiary hover:text-text-primary hover:bg-surface transition-colors"
          >
            <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M8.625 12a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Zm0 0H8.25m4.125 0a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Zm0 0H12m4.125 0a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Zm0 0h-.375M21 12c0 4.556-4.03 8.25-9 8.25a9.764 9.764 0 0 1-2.555-.337A5.972 5.972 0 0 1 5.41 20.97a5.969 5.969 0 0 1-.474-.065 4.48 4.48 0 0 0 .978-2.025c.09-.457-.133-.901-.467-1.226C3.93 16.178 3 14.189 3 12c0-4.556 4.03-8.25 9-8.25s9 3.694 9 8.25Z" />
            </svg>
          </button>

          {/* Admin */}
          <Link
            to="/admin"
            title="Instellingen"
            className="p-2 rounded-lg text-text-tertiary hover:text-text-primary hover:bg-surface transition-colors"
          >
            <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.723 7.723 0 0 1 0 .255c-.008.378.137.75.43.991l1.004.827c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 0 1-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 0 1-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 0 1-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 0 1-1.369-.49l-1.297-2.247a1.125 1.125 0 0 1 .26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 0 1 0-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 0 1-.26-1.43l1.297-2.247a1.125 1.125 0 0 1 1.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28Z" />
              <path strokeLinecap="round" strokeLinejoin="round" d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" />
            </svg>
          </Link>
        </div>
      </header>

      {/* Card layout */}
      <div className="flex-1 min-h-0 flex gap-4 px-4 pb-4 md:px-6 md:pb-6">
        {/* Main chat card */}
        <div className="flex-1 min-w-0 bg-surface rounded-2xl shadow-sm border border-border overflow-hidden">
          <ChatContainer
            messages={chat.messages}
            send={chat.send}
            stop={chat.stop}
            isLoading={chat.isLoading}
            chatMode="inline"
            scrubEnabled={scrubEnabled}
            onScrubSent={handleScrubSent}
            selectedMessageId={activeMessageId}
            onSelectMessage={setSelectedMessageId}
            onFeedback={handleFeedback}
            onAddToEval={setEvalModalMessageId}
          />
        </div>

        {/* Reasoning card — hidden on mobile */}
        <div className="hidden lg:block w-80 flex-shrink-0 bg-surface rounded-2xl shadow-sm border border-border overflow-hidden">
          <ReasoningSidebar
            message={selectedMessage}
            userQuestion={selectedUserQuestion}
          />
        </div>
      </div>

      {/* Session modal */}
      {sessionModalOpen && (
        <SessionModal
          activeSessionId={chat.sessionId}
          onSelectSession={handleSelectSession}
          onNewSession={handleNewSession}
          onClose={() => setSessionModalOpen(false)}
        />
      )}

      {/* Add to eval modal */}
      {evalModalMessageId && (() => {
        const evalMsg = chat.messages.find((m) => m.id === evalModalMessageId);
        const evalQuestion = evalMsg
          ? chat.messages.slice(0, chat.messages.indexOf(evalMsg)).reverse().find((m) => m.role === "user")?.content
          : undefined;
        if (!evalMsg?.structuredAnswer || !evalQuestion) return null;
        return (
          <AddToEvalModal
            userQuestion={evalQuestion}
            answer={evalMsg.structuredAnswer}
            onClose={() => setEvalModalMessageId(undefined)}
          />
        );
      })()}
    </div>
  );
}

function TeunAvatar({ size }: { size: number }) {
  return (
    <svg className="shrink-0" width={size} height={size} viewBox="0 0 200 200" fill="none">
      <path d="M32 8 H168 C182 8 192 18 192 32 V128 C192 142 182 152 168 152 H56 L24 180 V152 C14 152 8 142 8 128 V32 C8 18 18 8 32 8 Z" fill="url(#lgs-h)" />
      <rect x="36" y="34" width="58" height="72" rx="8" fill="white" />
      <rect x="50" y="54" width="30" height="4" rx="2" fill="#00D696" opacity="0.45" />
      <rect x="50" y="66" width="22" height="4" rx="2" fill="#00D696" opacity="0.3" />
      <rect x="50" y="78" width="26" height="4" rx="2" fill="#00D696" opacity="0.45" />
      <circle cx="128" cy="74" r="28" fill="white" />
      <circle cx="124" cy="70" r="15" fill="none" stroke="#00D696" strokeWidth="4.5" />
      <line x1="135" y1="81" x2="146" y2="92" stroke="#00D696" strokeWidth="4.5" strokeLinecap="round" />
      <defs>
        <linearGradient id="lgs-h" x1="8" y1="8" x2="192" y2="180" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#00D696" />
          <stop offset="100%" stopColor="#00C48A" />
        </linearGradient>
      </defs>
    </svg>
  );
}
