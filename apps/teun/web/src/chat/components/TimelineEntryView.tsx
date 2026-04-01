import { useState } from "react";
import type { TimelineEntry } from "../lib/types";

interface TimelineEntryViewProps {
  entry: TimelineEntry;
  isActive: boolean;
}

export function TimelineEntryView({ entry, isActive }: TimelineEntryViewProps) {
  const [expanded, setExpanded] = useState(false);

  // Partial: text fragment row
  if (entry.type === "partial") {
    return (
      <div className="text-text-tertiary">
        {entry.data.content}
        {isActive && (
          <span className="inline-block w-0.5 h-3 bg-accent ml-0.5 align-text-bottom timeline-dot-active" />
        )}
      </div>
    );
  }

  // Thinking: italic text row
  if (entry.type === "thinking") {
    return (
      <div className="italic text-text-tertiary">
        {entry.data.content ?? "Nadenken..."}
      </div>
    );
  }

  // Tool use: chip row with dot
  const label = entry.data.toolLabel
    ?? entry.data.tool?.replace(/^mcp__[^_]+__/, "").replace(/_/g, " ")
    ?? "Tool";

  return (
    <div>
      <button
        onClick={() => setExpanded(!expanded)}
        className="inline-flex items-center gap-1 rounded-md border border-border bg-surface px-1.5 py-0.5 text-text-secondary hover:text-text-primary hover:border-accent/30 transition-colors cursor-pointer"
      >
        {isActive ? (
          <svg className="h-3 w-3 timeline-spinner text-accent" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="3" opacity="0.2" />
            <path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
          </svg>
        ) : (
          <span className="h-1.5 w-1.5 rounded-full bg-timeline-dot flex-shrink-0" />
        )}
        <span>{label}</span>
        <svg
          className={`h-2.5 w-2.5 transition-transform ${expanded ? "rotate-90" : ""}`}
          fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor"
        >
          <path strokeLinecap="round" strokeLinejoin="round" d="m8.25 4.5 7.5 7.5-7.5 7.5" />
        </svg>
      </button>
      {expanded && entry.data.input != null && (
        <div className="mt-1 mb-1 p-2 rounded-md bg-border-light text-text-secondary font-mono text-[10px] whitespace-pre-wrap overflow-x-auto">
          {JSON.stringify(entry.data.input, null, 2)}
        </div>
      )}
    </div>
  );
}
