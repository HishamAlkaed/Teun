import { useState } from "react";
import type { ChatMessage } from "../lib/types";
import { JudgePanel } from "./JudgePanel";
import { SourceReference } from "./SourceReference";
import { AddToEvalModal } from "./AddToEvalModal";

interface ReasoningSidebarProps {
  message: ChatMessage | null;
  userQuestion?: string;
}

export function ReasoningSidebar({ message, userQuestion }: ReasoningSidebarProps) {
  const [evalModalOpen, setEvalModalOpen] = useState(false);
  const [stepsExpanded, setStepsExpanded] = useState(false);

  if (!message) {
    return (
      <aside className="h-full flex flex-col">
        <div className="p-4 border-b border-border">
          <div className="flex items-center gap-2">
            <TeunBrainIcon />
            <div>
              <div className="text-sm font-semibold text-text-primary">Het brein van Teun</div>
              <div className="text-[11px] text-text-tertiary">Zo komt het antwoord tot stand</div>
            </div>
          </div>
        </div>
        <div className="flex-1 flex items-center justify-center p-6">
          <p className="text-xs text-text-tertiary text-center">
            Stel een vraag om het denkproces van Teun te zien
          </p>
        </div>
      </aside>
    );
  }

  const timeline = message.timeline ?? [];
  const isStreaming = message.isStreaming ?? false;
  const answer = message.structuredAnswer;
  const judgeResult = message.judgeResult;

  // Build reasoning steps from thinking + tool_use entries
  const toolDescriptions: Record<string, string> = {
    "grep": "Zoeken in documenten",
    "glob": "Bestanden doorzoeken",
    "read": "Document lezen",
    "list directory": "Mappen bekijken",
    "search": "Zoeken",
    "bash": "Commando uitvoeren",
  };

  const describeToolUse = (toolLabel: string, input: unknown): string => {
    const lower = toolLabel.toLowerCase();
    const inp = input as Record<string, unknown> | undefined;
    let base = toolLabel.charAt(0).toUpperCase() + toolLabel.slice(1);
    for (const [key, desc] of Object.entries(toolDescriptions)) {
      if (lower.includes(key)) { base = desc; break; }
    }

    // Add a short detail from the input
    if (inp && typeof inp === "object") {
      if (typeof inp.pattern === "string") return `${base}: "${inp.pattern}"`;
      if (typeof inp.query === "string") return `${base}: "${inp.query}"`;
      if (typeof inp.file_path === "string") {
        const filename = (inp.file_path as string).split("/").pop();
        return `${base}: ${filename}`;
      }
      if (typeof inp.path === "string") {
        const filename = (inp.path as string).split("/").pop();
        return `${base}: ${filename}`;
      }
    }

    return base;
  };

  const steps: { label: string; isActive?: boolean }[] = [];
  for (const entry of timeline) {
    if (entry.type === "thinking" && entry.data.content) {
      const text = entry.data.content.length > 200
        ? entry.data.content.slice(0, 200) + "..."
        : entry.data.content;
      steps.push({ label: text });
    } else if (entry.type === "tool_use") {
      steps.push({ label: describeToolUse(entry.data.toolLabel as string ?? "", entry.data.input) });
    }
  }

  // If we have an answer, add conclusion step
  if (answer) {
    const conclusionLabel = answer.category === "doorverwijzen_speciale_afhandeling"
      ? "Conclusie: doorverwijzen"
      : "Conclusie: standaard antwoord";
    steps.push({ label: conclusionLabel });
  }

  // If streaming and no steps yet
  if (isStreaming && steps.length === 0) {
    steps.push({ label: "Aan het nadenken...", isActive: true });
  }

  // Auto-collapse steps once the answer is available
  const hasAnswer = !!answer;
  const showStepsExpanded = !hasAnswer || stepsExpanded;

  return (
    <aside className="h-full flex flex-col">
      {/* Header */}
      <div className="p-4 border-b border-border">
        <div className="flex items-center gap-2">
          <TeunBrainIcon />
          <div>
            <div className="text-sm font-semibold text-text-primary">Het brein van Teun</div>
            <div className="text-[11px] text-text-tertiary">Zo komt het antwoord tot stand</div>
          </div>
        </div>
      </div>

      {/* Scrollable content */}
      <div className="flex-1 overflow-y-auto p-4 space-y-5">
        {/* Reasoning steps */}
        {steps.length > 0 && (
          <div className="space-y-0">
            {/* Collapsed summary */}
            {hasAnswer && !showStepsExpanded ? (
              <button
                onClick={() => setStepsExpanded(true)}
                className="flex items-center gap-2 text-xs text-text-tertiary hover:text-text-secondary transition-colors cursor-pointer w-full"
              >
                <svg className="w-3.5 h-3.5 flex-shrink-0" viewBox="0 0 20 20" fill="currentColor">
                  <path fillRule="evenodd" d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z" clipRule="evenodd" />
                </svg>
                <span>{steps.length} denkstappen</span>
              </button>
            ) : (
              <>
                {hasAnswer && (
                  <button
                    onClick={() => setStepsExpanded(false)}
                    className="flex items-center gap-2 text-xs text-text-tertiary hover:text-text-secondary transition-colors cursor-pointer w-full mb-2"
                  >
                    <svg className="w-3.5 h-3.5 flex-shrink-0" viewBox="0 0 20 20" fill="currentColor">
                      <path fillRule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clipRule="evenodd" />
                    </svg>
                    <span>{steps.length} denkstappen</span>
                  </button>
                )}
                {steps.map((step, i) => (
                  <div key={i} className="flex gap-3">
                    {/* Step number with vertical line */}
                    <div className="flex flex-col items-center">
                      <div className={`w-5 h-5 rounded-full flex items-center justify-center text-[11px] font-bold flex-shrink-0 ${
                        step.isActive
                          ? "bg-accent/20 text-accent"
                          : "bg-accent text-white"
                      }`}>
                        {step.isActive ? (
                          <svg className="h-3 w-3 timeline-spinner" viewBox="0 0 24 24" fill="none">
                            <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="3" opacity="0.3" />
                            <path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
                          </svg>
                        ) : (
                          i + 1
                        )}
                      </div>
                      {i < steps.length - 1 && (
                        <div className="w-px flex-1 bg-border min-h-[12px]" />
                      )}
                    </div>
                    <div className="pb-3 pt-0.5">
                      <div className="text-xs text-text-primary leading-snug">{step.label}</div>
                    </div>
                  </div>
                ))}
              </>
            )}
          </div>
        )}

        {/* Rationale */}
        {answer?.rationale && (
          <div>
            <div className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider mb-2">
              Onderbouwing
            </div>
            <div className="text-xs text-text-secondary leading-relaxed whitespace-pre-wrap">
              {answer.rationale}
            </div>
          </div>
        )}

        {/* Sources */}
        {answer && answer.sources.length > 0 && (
          <div>
            <div className="flex items-center justify-between mb-2">
              <div className="text-[10px] font-semibold text-text-tertiary uppercase tracking-wider">
                Bronnen
              </div>
              {userQuestion && (
                <button
                  onClick={() => setEvalModalOpen(true)}
                  className="text-[11px] text-text-tertiary hover:text-accent transition-colors cursor-pointer"
                >
                  + Eval set
                </button>
              )}
            </div>
            <div className="flex flex-col gap-1.5">
              {answer.sources.map((source, i) => (
                <SourceReference key={i} source={source} />
              ))}
            </div>
          </div>
        )}

        {/* Judge result */}
        {judgeResult && <JudgePanel result={judgeResult} />}

        {/* Streaming judge indicator */}
        {isStreaming && answer && !judgeResult && (
          <div className="flex items-center gap-1.5 text-xs text-text-tertiary">
            <svg className="h-3 w-3 timeline-spinner" viewBox="0 0 24 24" fill="none">
              <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="2" opacity="0.2" />
              <path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            </svg>
            Kwaliteit controleren...
          </div>
        )}
      </div>

      {evalModalOpen && userQuestion && answer && (
        <AddToEvalModal
          userQuestion={userQuestion}
          answer={answer}
          onClose={() => setEvalModalOpen(false)}
        />
      )}
    </aside>
  );
}

function TeunBrainIcon() {
  return (
    <svg className="w-8 h-8 text-accent flex-shrink-0" viewBox="0 0 200 200" fill="none">
      <path d="M32 8 H168 C182 8 192 18 192 32 V128 C192 142 182 152 168 152 H56 L24 180 V152 C14 152 8 142 8 128 V32 C8 18 18 8 32 8 Z" fill="currentColor" />
      <circle cx="100" cy="80" r="35" fill="white" opacity="0.9" />
      <path d="M85 80 C85 72 92 65 100 65 C108 65 115 72 115 80 C115 88 108 95 100 95" stroke="currentColor" strokeWidth="5" fill="none" strokeLinecap="round" />
      <circle cx="100" cy="105" r="3" fill="currentColor" />
    </svg>
  );
}
