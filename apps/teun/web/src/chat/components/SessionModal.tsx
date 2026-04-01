import { useEffect, useState } from "react";
import { deleteSession, listSessions } from "../lib/api";
import type { StoredSession } from "../lib/types";

interface SessionModalProps {
  activeSessionId?: string;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
  onClose: () => void;
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

export function SessionModal({ activeSessionId, onSelectSession, onNewSession, onClose }: SessionModalProps) {
  const [sessions, setSessions] = useState<StoredSession[]>([]);

  useEffect(() => {
    listSessions().then(setSessions).catch(() => {});
  }, []);

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

  const handleSelect = (id: string) => {
    onSelectSession(id);
    onClose();
  };

  const handleNew = () => {
    onNewSession();
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30" onClick={onClose}>
      <div
        className="bg-surface rounded-2xl shadow-xl w-full max-w-md max-h-[70vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-4 border-b border-border flex items-center justify-between">
          <h2 className="text-sm font-semibold text-text-primary">Gesprekken</h2>
          <button onClick={onClose} className="text-text-tertiary hover:text-text-primary transition-colors">
            <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="p-3">
          <button
            onClick={handleNew}
            className="w-full flex items-center justify-center gap-2 px-3 py-2 text-sm font-medium text-text-primary bg-page-bg rounded-lg border border-border hover:border-accent/30 hover:text-accent transition-colors cursor-pointer"
          >
            <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
            </svg>
            Nieuw gesprek
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-3 pb-3">
          {sessions.length === 0 && (
            <div className="py-8 text-xs text-text-tertiary text-center">
              Geen eerdere gesprekken
            </div>
          )}
          <div className="space-y-1">
            {sessions.map((session) => {
              const isActive = session.id === activeSessionId;
              return (
                <button
                  key={session.id}
                  onClick={() => handleSelect(session.id)}
                  className={`w-full text-left px-3 py-2.5 rounded-lg transition-colors cursor-pointer group ${
                    isActive
                      ? "bg-accent/10 text-accent"
                      : "text-text-secondary hover:bg-page-bg hover:text-text-primary"
                  }`}
                >
                  <div className="flex items-start justify-between gap-2">
                    <span className="text-sm truncate leading-snug flex-1">
                      {session.title || "Zonder titel"}
                    </span>
                    <button
                      onClick={(e) => handleDelete(e, session.id)}
                      className="opacity-0 group-hover:opacity-100 p-0.5 text-text-tertiary hover:text-red-500 transition-all cursor-pointer flex-shrink-0"
                      title="Verwijderen"
                    >
                      <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
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
        </div>
      </div>
    </div>
  );
}
