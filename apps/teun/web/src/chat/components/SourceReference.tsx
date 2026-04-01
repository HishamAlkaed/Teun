import { useState } from "react";
import type { SourceReference as SourceRef } from "../lib/types";
import { DocumentViewer } from "./DocumentViewer";

interface SourceReferenceProps {
  source: SourceRef;
}

export function SourceReference({ source }: SourceReferenceProps) {
  const [expanded, setExpanded] = useState(false);
  const [viewerOpen, setViewerOpen] = useState(false);
  const hasQuote = !!source.quote;

  return (
    <>
      <div className="rounded-md border border-border bg-surface text-xs">
        <div className="flex items-start gap-1.5 w-full px-2.5 py-1.5 text-left text-text-secondary min-w-0">
          <svg className="h-3 w-3 text-text-tertiary flex-shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 0 0-3.375-3.375h-1.5A1.125 1.125 0 0 1 13.5 7.125v-1.5a3.375 3.375 0 0 0-3.375-3.375H8.25m0 12.75h7.5m-7.5 3H12M10.5 2.25H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 0 0-9-9Z" />
          </svg>
          <div className="min-w-0 flex-1">
            <button
              onClick={() => setViewerOpen(true)}
              className="font-medium text-accent hover:underline truncate block text-left cursor-pointer"
              title="Open document"
            >
              {source.document.replace(/\.md$/, "")}
            </button>
            <div className="flex items-center gap-1.5 text-text-tertiary mt-0.5">
              {source.section && <span className="truncate">{source.section}</span>}
              {source.line_range && (
                <button
                  onClick={() => setViewerOpen(true)}
                  className="font-mono flex-shrink-0 hover:text-accent cursor-pointer"
                  title="Ga naar regel"
                >
                  r. {source.line_range}
                </button>
              )}
            </div>
          </div>
          {hasQuote && (
            <button
              onClick={() => setExpanded(!expanded)}
              className="p-0.5 hover:text-text-primary transition-colors cursor-pointer flex-shrink-0"
              title={expanded ? "Citaat verbergen" : "Citaat tonen"}
            >
              <svg
                className={`h-2.5 w-2.5 transition-transform ${expanded ? "rotate-90" : ""}`}
                fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor"
              >
                <path strokeLinecap="round" strokeLinejoin="round" d="m8.25 4.5 7.5 7.5-7.5 7.5" />
              </svg>
            </button>
          )}
        </div>
        {expanded && source.quote && (
          <div className="px-2.5 pb-2 pt-0">
            <div className="p-2 rounded bg-border-light text-text-secondary text-[11px] leading-relaxed whitespace-pre-wrap border-l-2 border-accent/30">
              {source.quote}
            </div>
          </div>
        )}
      </div>

      {viewerOpen && (
        <DocumentViewer
          filename={source.document}
          lineRange={source.line_range}
          section={source.section}
          onClose={() => setViewerOpen(false)}
        />
      )}
    </>
  );
}
