import { useState } from "react";
import type { MortgageAnswer } from "../lib/types";
import { createQuestion } from "../../lib/api";

interface AddToEvalModalProps {
  userQuestion: string;
  answer: MortgageAnswer;
  onClose: () => void;
}

const categoryOptions = [
  { value: "standard", label: "Standaard" },
  { value: "doorverwijzen_speciale_afhandeling", label: "Doorverwijzen / speciale afhandeling" },
];

export function AddToEvalModal({ userQuestion, answer, onClose }: AddToEvalModalProps) {
  const [question, setQuestion] = useState(userQuestion);
  const [category, setCategory] = useState(answer.category);
  const [keyPoints, setKeyPoints] = useState(answer.answer.slice(0, 200));
  const [description, setDescription] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSave = async () => {
    if (!question.trim()) return;
    setSaving(true);
    setError(null);
    try {
      await createQuestion({
        question: question.trim(),
        expected_category: category,
        expected_key_points: keyPoints
          .split("\n")
          .map((l) => l.trim())
          .filter(Boolean),
        description: description.trim() || null,
        points: 1,
      });
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Opslaan mislukt");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/40" onClick={onClose} />
      <div className="relative bg-page-bg border border-border rounded-lg shadow-xl w-full max-w-lg mx-4 max-h-[90vh] overflow-y-auto">
        <div className="p-5 space-y-4">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-text-primary">Toevoegen aan evaluatieset</h3>
            <button
              onClick={onClose}
              className="text-text-tertiary hover:text-text-primary transition-colors"
            >
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">Vraag</label>
            <textarea
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              className="w-full text-sm rounded-md border border-border bg-surface p-2 text-text-primary placeholder:text-text-tertiary focus:outline-none focus:border-accent/50 resize-y min-h-[60px]"
              rows={2}
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">Verwachte categorie</label>
            <select
              value={category}
              onChange={(e) => setCategory(e.target.value as MortgageAnswer["category"])}
              className="w-full text-sm rounded-md border border-border bg-surface p-2 text-text-primary focus:outline-none focus:border-accent/50"
            >
              {categoryOptions.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">
              Verwachte kernpunten <span className="font-normal text-text-tertiary">(een per regel)</span>
            </label>
            <textarea
              value={keyPoints}
              onChange={(e) => setKeyPoints(e.target.value)}
              className="w-full text-sm rounded-md border border-border bg-surface p-2 text-text-primary placeholder:text-text-tertiary focus:outline-none focus:border-accent/50 resize-y min-h-[80px]"
              rows={4}
              placeholder="Kernpunt 1&#10;Kernpunt 2"
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">
              Beschrijving <span className="font-normal text-text-tertiary">(optioneel)</span>
            </label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="w-full text-sm rounded-md border border-border bg-surface p-2 text-text-primary placeholder:text-text-tertiary focus:outline-none focus:border-accent/50 resize-y"
              rows={2}
              placeholder="Bijv. context over waarom deze vraag relevant is"
            />
          </div>

          {error && (
            <div className="p-2 rounded-md bg-red-50 border border-red-100 text-xs text-red-700">
              {error}
            </div>
          )}

          <div className="flex justify-end gap-2 pt-1">
            <button
              onClick={onClose}
              className="text-xs rounded-md border border-border px-3 py-1.5 text-text-secondary hover:bg-surface transition-colors"
            >
              Annuleren
            </button>
            <button
              onClick={handleSave}
              disabled={saving || !question.trim()}
              className="text-xs rounded-md bg-accent text-white px-3 py-1.5 hover:bg-accent/90 transition-colors disabled:opacity-50"
            >
              {saving ? "Opslaan..." : "Toevoegen"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
