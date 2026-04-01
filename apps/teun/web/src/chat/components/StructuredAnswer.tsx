import { useState } from "react";
import Markdown from "react-markdown";
import type { JudgeResult, MessageFeedback, MortgageAnswer } from "../lib/types";
import { AddToEvalModal } from "./AddToEvalModal";
import { CategoryBadge } from "./CategoryBadge";
import { FeedbackPanel } from "./FeedbackPanel";
import { JudgePanel } from "./JudgePanel";
import { SourceReference } from "./SourceReference";

interface StructuredAnswerProps {
  answer: MortgageAnswer;
  judgeResult?: JudgeResult;
  feedback?: MessageFeedback;
  onFeedbackSubmit?: (feedback: MessageFeedback) => void;
  userQuestion?: string;
}

export function StructuredAnswer({ answer, judgeResult, feedback, onFeedbackSubmit, userQuestion }: StructuredAnswerProps) {
  const [rationaleOpen, setRationaleOpen] = useState(false);
  const [evalModalOpen, setEvalModalOpen] = useState(false);

  const hasSideContent =
    answer.rationale ||
    answer.sources.length > 0 ||
    judgeResult;

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-1 lg:grid-cols-[1fr_300px] gap-6">
        {/* Main column: answer */}
        <div>
          <CategoryBadge category={answer.category} />
          <div className="mt-3 text-text-primary text-base leading-relaxed prose prose-sm max-w-none prose-p:my-1.5 prose-ul:my-1.5 prose-ol:my-1.5 prose-li:my-0.5 prose-headings:my-2">
            <Markdown>{answer.answer}</Markdown>
          </div>
        </div>

        {/* Side column: rationale, sources, judge */}
        {hasSideContent && (
          <div className="bg-surface rounded-lg border border-border p-3 space-y-3 self-start min-w-0 overflow-hidden">
            {answer.rationale && (
              <div>
                <button
                  onClick={() => setRationaleOpen(!rationaleOpen)}
                  className="flex items-center gap-1.5 text-xs font-medium text-text-secondary hover:text-text-primary transition-colors"
                >
                  <svg
                    className={`h-3 w-3 transition-transform ${rationaleOpen ? "rotate-90" : ""}`}
                    fill="none"
                    viewBox="0 0 24 24"
                    strokeWidth={2}
                    stroke="currentColor"
                  >
                    <path strokeLinecap="round" strokeLinejoin="round" d="m8.25 4.5 7.5 7.5-7.5 7.5" />
                  </svg>
                  Redenering
                </button>
                {rationaleOpen && (
                  <div className="mt-1.5 pl-4 text-xs text-text-secondary leading-relaxed border-l-2 border-border whitespace-pre-wrap">
                    {answer.rationale}
                  </div>
                )}
              </div>
            )}

            {answer.sources.length > 0 && (
              <div>
                <div className="text-[10px] font-medium text-text-tertiary uppercase tracking-wider mb-1.5">
                  Bronnen
                </div>
                <div className="flex flex-col gap-1.5">
                  {answer.sources.map((source, i) => (
                    <SourceReference key={i} source={source} />
                  ))}
                </div>
              </div>
            )}

            {judgeResult && <JudgePanel result={judgeResult} />}
          </div>
        )}
      </div>

      {/* Feedback beneath the answer, full width */}
      {onFeedbackSubmit && (
        <div className="border-t border-border pt-3">
          <div className="flex items-center justify-between mb-2">
            <div className="text-xs font-medium text-text-tertiary uppercase tracking-wider">
              Beoordeling
            </div>
            {userQuestion && (
              <button
                onClick={() => setEvalModalOpen(true)}
                className="text-[11px] text-text-tertiary hover:text-accent transition-colors"
              >
                + Eval set
              </button>
            )}
          </div>
          <FeedbackPanel feedback={feedback} onSubmit={onFeedbackSubmit} />
        </div>
      )}

      {evalModalOpen && userQuestion && (
        <AddToEvalModal
          userQuestion={userQuestion}
          answer={answer}
          onClose={() => setEvalModalOpen(false)}
        />
      )}
    </div>
  );
}
