import { useEffect, useState } from "react";
import { deleteSession, listSessions } from "../lib/api";
import type { StoredSession } from "../lib/types";

interface SessionSidebarProps {
  activeSessionId?: string;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
  pendingSession?: { title: string };
  onPendingClick?: () => void;
}

function relativeTime(dateStr: string): string {
  const now = Date.now();
  const then = new Date(dateStr).getTime();
  const diff = now - then;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "zojuist";
  if (mins < 60) return `${mins}m geleden`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}u geleden`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d geleden`;
  return new Date(dateStr).toLocaleDateString("nl-NL", {
    day: "numeric",
    month: "short",
  });
}

export function SessionSidebar({
  activeSessionId,
  onSelectSession,
  onNewSession,
  pendingSession,
  onPendingClick,
}: SessionSidebarProps) {
  const [sessions, setSessions] = useState<StoredSession[]>([]);

  const refresh = async () => {
    try {
      const list = await listSessions();
      setSessions(list);
    } catch {
      // Silently fail — sidebar is non-critical
    }
  };

  useEffect(() => {
    void refresh();
    const interval = setInterval(() => void refresh(), 10000);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    void refresh();
  }, [activeSessionId]);

  // Show pending entry whenever we have an active chat without a sessionId yet.
  // It disappears naturally when pendingSession becomes undefined (sessionId arrives).
  const showPending = !!pendingSession;

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    try {
      await deleteSession(id);
      setSessions((prev) => prev.filter((s) => s.id !== id));
      if (id === activeSessionId) {
        onNewSession();
      }
    } catch {
      // Ignore
    }
  };

  return (
    <aside className="w-64 flex-shrink-0 border-r border-border bg-surface h-full flex flex-col">
      <div className="p-3 border-b border-border">
        <button
          onClick={onNewSession}
          className="w-full flex items-center justify-center gap-2 px-3 py-2 text-sm font-medium text-text-primary bg-page-bg rounded-lg border border-border hover:border-accent/30 hover:text-accent transition-colors cursor-pointer"
        >
          <svg
            className="h-4 w-4"
            fill="none"
            viewBox="0 0 24 24"
            strokeWidth={1.5}
            stroke="currentColor"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M12 4.5v15m7.5-7.5h-15"
            />
          </svg>
          Nieuw gesprek
        </button>
      </div>

      <div className="flex-1 overflow-y-auto">
        {showPending && (
          <button
            onClick={onPendingClick}
            className={`w-full text-left px-3 py-2.5 border-b border-border-light cursor-pointer transition-colors ${
              !activeSessionId
                ? "bg-accent-light text-accent"
                : "text-text-secondary hover:bg-page-bg hover:text-text-primary"
            }`}
          >
            <span className="text-sm truncate leading-snug block">
              {pendingSession.title || "Nieuw gesprek"}
            </span>
            <div className="text-[10px] text-text-tertiary mt-0.5 flex items-center gap-1">
              <svg className="h-3 w-3 animate-spin" fill="none" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
              </svg>
              wordt gegenereerd...
            </div>
          </button>
        )}

        {!showPending && sessions.length === 0 && (
          <div className="px-3 py-6 text-xs text-text-tertiary text-center">
            Geen eerdere gesprekken
          </div>
        )}

        {sessions.map((session) => {
          const isActive = session.id === activeSessionId;
          return (
            <button
              key={session.id}
              onClick={() => onSelectSession(session.id)}
              className={`w-full text-left px-3 py-2.5 border-b border-border-light transition-colors cursor-pointer group ${
                isActive
                  ? "bg-accent-light text-accent"
                  : "text-text-secondary hover:bg-page-bg hover:text-text-primary"
              }`}
            >
              <div className="flex items-start justify-between gap-1">
                <span className="text-sm truncate leading-snug flex-1">
                  {session.title || "Zonder titel"}
                </span>
                <button
                  onClick={(e) => handleDelete(e, session.id)}
                  className="opacity-0 group-hover:opacity-100 p-0.5 text-text-tertiary hover:text-red-500 transition-all cursor-pointer flex-shrink-0"
                  title="Verwijderen"
                >
                  <svg
                    className="h-3.5 w-3.5"
                    fill="none"
                    viewBox="0 0 24 24"
                    strokeWidth={1.5}
                    stroke="currentColor"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0"
                    />
                  </svg>
                </button>
              </div>
              <div className="text-[10px] text-text-tertiary mt-0.5">
                {relativeTime(session.last_active)}
              </div>
            </button>
          );
        })}
      </div>
    </aside>
  );
}
