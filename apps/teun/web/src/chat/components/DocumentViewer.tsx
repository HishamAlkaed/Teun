import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";

interface DocumentViewerProps {
  filename: string;
  lineRange?: string;
  section?: string;
  onClose: () => void;
}

interface DocumentData {
  filename: string;
  total_lines: number;
  content: string;
  highlight_start: number | null;
  highlight_end: number | null;
  highlight_out_of_bounds?: boolean;
  /** Additive backend field: "pdf_text" (DB extracted text) | "markdown" (disk .md). */
  content_type?: string;
}

export function DocumentViewer({ filename, lineRange, section, onClose }: DocumentViewerProps) {
  const [doc, setDoc] = useState<DocumentData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"rendered" | "lines">("rendered");
  const highlightRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const params = new URLSearchParams();
    if (lineRange) params.set("line_range", lineRange);
    const qs = params.toString();
    const url = `/api/teun/documents/${encodeURIComponent(filename)}${qs ? `?${qs}` : ""}`;

    fetch(url)
      .then(async (res) => {
        if (!res.ok) {
          const data = await res.json().catch(() => ({}));
          throw new Error(data.error || "Kon document niet laden");
        }
        return res.json() as Promise<DocumentData>;
      })
      .then((data) => {
        setDoc(data);
        // PDF-extracted text is not markdown: default to the line view so the
        // cited range is readable. The user can still toggle to "Opgemaakt".
        if (data.content_type === "pdf_text") {
          setViewMode("lines");
        }
      })
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, [filename, lineRange]);

  // Scroll to highlighted section after render
  useEffect(() => {
    if (doc && highlightRef.current) {
      setTimeout(() => {
        highlightRef.current?.scrollIntoView({ behavior: "smooth", block: "center" });
      }, 100);
    }
  }, [doc, viewMode]);

  // Pretty document name (remove .md extension)
  const displayName = filename.replace(/\.md$/, "");

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={onClose} />
      <div className="relative bg-page-bg border border-border rounded-lg shadow-xl w-full max-w-4xl mx-4 max-h-[90vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-border flex-shrink-0">
          <div className="min-w-0">
            <h3 className="text-sm font-medium text-text-primary truncate">{displayName}</h3>
            <div className="flex items-center gap-2 text-[11px] text-text-tertiary mt-0.5">
              {section && <span>{section}</span>}
              {lineRange && <span className="font-mono">Regel {lineRange}</span>}
              {doc && <span>{doc.total_lines} regels</span>}
            </div>
          </div>
          <div className="flex items-center gap-2 flex-shrink-0">
            <div className="flex rounded-md border border-border overflow-hidden">
              <button
                onClick={() => setViewMode("rendered")}
                className={`px-2.5 py-1 text-[11px] transition-colors ${viewMode === "rendered" ? "bg-accent text-white" : "text-text-secondary hover:bg-surface"}`}
              >
                Opgemaakt
              </button>
              <button
                onClick={() => setViewMode("lines")}
                className={`px-2.5 py-1 text-[11px] transition-colors ${viewMode === "lines" ? "bg-accent text-white" : "text-text-secondary hover:bg-surface"}`}
              >
                Regelnummers
              </button>
            </div>
            <button
              onClick={onClose}
              className="text-text-tertiary hover:text-text-primary transition-colors p-1"
            >
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto min-h-0">
          {loading && (
            <div className="flex items-center justify-center py-12 text-text-tertiary text-xs">
              Document laden...
            </div>
          )}

          {error && (
            <div className="p-5 text-xs text-red-700">{error}</div>
          )}

          {doc?.highlight_out_of_bounds && lineRange && (
            <div className="mx-5 mt-4 px-3 py-2 text-[11px] text-amber-800 bg-amber-50 border border-amber-200 rounded">
              De geciteerde regels ({lineRange}) bestaan niet in dit document
              (totaal {doc.total_lines} regels). Het document is waarschijnlijk
              bijgewerkt sinds dit antwoord werd gegenereerd. Zoek de sectie
              {section ? ` "${section}" ` : " "}hieronder handmatig.
            </div>
          )}

          {doc && viewMode === "rendered" && (
            <RenderedView
              content={doc.content}
              highlightStart={doc.highlight_start}
              highlightEnd={doc.highlight_end}
              highlightRef={highlightRef}
            />
          )}

          {doc && viewMode === "lines" && (
            <LineNumberView
              content={doc.content}
              highlightStart={doc.highlight_start}
              highlightEnd={doc.highlight_end}
              highlightRef={highlightRef}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function RenderedView({
  content,
  highlightStart,
  highlightEnd,
  highlightRef,
}: {
  content: string;
  highlightStart: number | null;
  highlightEnd: number | null;
  highlightRef: React.RefObject<HTMLDivElement | null>;
}) {
  // If we have a highlight range, split the content into before/highlighted/after
  if (highlightStart && highlightEnd) {
    const lines = content.split("\n");
    const before = lines.slice(0, highlightStart - 1).join("\n");
    const highlighted = lines.slice(highlightStart - 1, highlightEnd).join("\n");
    const after = lines.slice(highlightEnd).join("\n");

    return (
      <div className="p-5 prose prose-sm max-w-none prose-p:my-1.5 prose-ul:my-1.5 prose-ol:my-1.5 prose-li:my-0.5 prose-headings:my-2 text-text-primary">
        {before && <Markdown>{before}</Markdown>}
        <div
          ref={highlightRef}
          className="bg-yellow-100 border-l-4 border-yellow-400 -mx-2 px-2 py-1 rounded-r"
        >
          <Markdown>{highlighted}</Markdown>
        </div>
        {after && <Markdown>{after}</Markdown>}
      </div>
    );
  }

  return (
    <div className="p-5 prose prose-sm max-w-none prose-p:my-1.5 prose-ul:my-1.5 prose-ol:my-1.5 prose-li:my-0.5 prose-headings:my-2 text-text-primary">
      <Markdown>{content}</Markdown>
    </div>
  );
}

function LineNumberView({
  content,
  highlightStart,
  highlightEnd,
  highlightRef,
}: {
  content: string;
  highlightStart: number | null;
  highlightEnd: number | null;
  highlightRef: React.RefObject<HTMLDivElement | null>;
}) {
  const lines = content.split("\n");

  return (
    <div className="font-mono text-[11px] leading-5">
      {lines.map((line, i) => {
        const lineNum = i + 1;
        const isHighlighted =
          highlightStart !== null &&
          highlightEnd !== null &&
          lineNum >= highlightStart &&
          lineNum <= highlightEnd;

        return (
          <div
            key={i}
            ref={isHighlighted && lineNum === highlightStart ? highlightRef : undefined}
            className={`flex ${isHighlighted ? "bg-yellow-100" : "hover:bg-surface-hover"}`}
          >
            <span className="w-12 flex-shrink-0 text-right pr-3 text-text-tertiary select-none border-r border-border-light">
              {lineNum}
            </span>
            <span className="pl-3 pr-5 whitespace-pre-wrap break-all text-text-primary">
              {line || "\u00A0"}
            </span>
          </div>
        );
      })}
    </div>
  );
}
