import { useState } from "react";
import type { MessageFeedback } from "../lib/types";

interface FeedbackPanelProps {
  feedback?: MessageFeedback;
  onSubmit: (feedback: MessageFeedback) => void;
  compact?: boolean;
}

const statuses = [
  { key: "approved", label: "Goedkeuren", icon: "\u2713", color: "text-green-600 bg-green-50 border-green-200 hover:bg-green-100" },
  { key: "partial", label: "Deels correct", icon: "\u00BD", color: "text-amber-600 bg-amber-50 border-amber-200 hover:bg-amber-100" },
  { key: "rejected", label: "Afkeuren", icon: "\u2717", color: "text-red-600 bg-red-50 border-red-200 hover:bg-red-100" },
] as const;

export function FeedbackPanel({ feedback, onSubmit, compact }: FeedbackPanelProps) {
  const [selected, setSelected] = useState<string | undefined>(feedback?.status);
  const [comment, setComment] = useState(feedback?.comment ?? "");
  const [submitted, setSubmitted] = useState(!!feedback);

  const needsComment = selected === "partial" || selected === "rejected";

  const handleSubmit = () => {
    if (!selected) return;
    onSubmit({
      status: selected as MessageFeedback["status"],
      comment: needsComment && comment.trim() ? comment.trim() : undefined,
    });
    setSubmitted(true);
  };

  if (submitted && feedback) {
    const statusInfo = statuses.find((s) => s.key === feedback.status);
    return (
      <div className="flex items-center gap-2 text-xs text-text-secondary">
        <span className={`inline-flex items-center gap-1 rounded-md border px-2 py-0.5 ${statusInfo?.color ?? ""}`}>
          {statusInfo?.label ?? feedback.status}
        </span>
        {feedback.comment && (
          <span className="text-text-tertiary italic">{feedback.comment}</span>
        )}
        <button
          onClick={() => setSubmitted(false)}
          className="text-text-tertiary hover:text-text-primary transition-colors ml-1"
        >
          Wijzig
        </button>
      </div>
    );
  }

  if (compact) {
    return (
      <div className="space-y-1.5">
        <div className="grid grid-cols-3 gap-1.5">
          {statuses.map((s) => (
            <button
              key={s.key}
              onClick={() => { setSelected(s.key); setSubmitted(false); }}
              className={`text-[11px] rounded-md border px-1.5 py-1 transition-colors text-center ${
                selected === s.key
                  ? s.color + " font-medium"
                  : "text-text-secondary border-border hover:border-accent/30"
              }`}
            >
              {s.label}
            </button>
          ))}
        </div>
        {needsComment && (
          <textarea
            value={comment}
            onChange={(e) => setComment(e.target.value)}
            placeholder="Toelichting..."
            className="w-full text-[11px] rounded-md border border-border bg-surface p-1.5 text-text-primary placeholder:text-text-tertiary focus:outline-none focus:border-accent/50 resize-y min-h-[40px]"
            rows={2}
          />
        )}
        {selected && (
          <button
            onClick={handleSubmit}
            className="text-[11px] rounded-md bg-accent text-white px-2.5 py-0.5 hover:bg-accent/90 transition-colors"
          >
            Opslaan
          </button>
        )}
      </div>
    );
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        {statuses.map((s) => (
          <button
            key={s.key}
            onClick={() => { setSelected(s.key); setSubmitted(false); }}
            className={`text-xs rounded-md border px-2 py-0.5 transition-colors ${
              selected === s.key
                ? s.color + " font-medium"
                : "text-text-secondary border-border hover:border-accent/30"
            }`}
          >
            {s.label}
          </button>
        ))}
      </div>
      {needsComment && (
        <textarea
          value={comment}
          onChange={(e) => setComment(e.target.value)}
          placeholder="Toelichting: wat moet het antwoord wel zijn?"
          className="w-full text-xs rounded-md border border-border bg-surface p-2 text-text-primary placeholder:text-text-tertiary focus:outline-none focus:border-accent/50 resize-y min-h-[60px]"
          rows={2}
        />
      )}
      {selected && (
        <button
          onClick={handleSubmit}
          className="text-xs rounded-md bg-accent text-white px-3 py-1 hover:bg-accent/90 transition-colors"
        >
          Opslaan
        </button>
      )}
    </div>
  );
}
