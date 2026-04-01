import { useCallback, useEffect, useState } from "react";
import {
  type TestQuestion,
  type QuestionPayload,
  listQuestions,
  createQuestion,
  updateQuestion,
  deleteQuestion,
} from "../lib/api";

const emptyPayload: QuestionPayload = {
  question: "",
  expected_category: null,
  expected_key_points: [],
  description: null,
  points: 1,
};

export function QuestionsTab() {
  const [questions, setQuestions] = useState<TestQuestion[]>([]);
  const [loading, setLoading] = useState(true);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [form, setForm] = useState<QuestionPayload>(emptyPayload);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);

  const load = useCallback(() => {
    setLoading(true);
    void listQuestions()
      .then(setQuestions)
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const startCreate = () => {
    setEditingId(null);
    setForm(emptyPayload);
    setCreating(true);
    setError(null);
  };

  const startEdit = (q: TestQuestion) => {
    setCreating(false);
    setEditingId(q.id);
    setForm({
      question: q.question,
      expected_category: q.expected_category,
      expected_key_points: q.expected_key_points,
      description: q.description,
      points: q.points,
    });
    setError(null);
  };

  const cancel = () => {
    setCreating(false);
    setEditingId(null);
    setError(null);
  };

  const handleSave = async () => {
    if (!form.question.trim()) {
      setError("Vraag mag niet leeg zijn");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      if (creating) {
        await createQuestion(form);
      } else if (editingId) {
        await updateQuestion(editingId, form);
      }
      cancel();
      load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Opslaan mislukt");
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteQuestion(id);
      setDeleteConfirm(null);
      load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Verwijderen mislukt");
    }
  };

  const addKeyPoint = () => {
    setForm((f) => ({
      ...f,
      expected_key_points: [...f.expected_key_points, ""],
    }));
  };

  const updateKeyPoint = (index: number, value: string) => {
    setForm((f) => ({
      ...f,
      expected_key_points: f.expected_key_points.map((kp, i) =>
        i === index ? value : kp
      ),
    }));
  };

  const removeKeyPoint = (index: number) => {
    setForm((f) => ({
      ...f,
      expected_key_points: f.expected_key_points.filter((_, i) => i !== index),
    }));
  };

  if (loading) {
    return <div className="text-xs text-text-tertiary">Laden...</div>;
  }

  const isEditing = creating || editingId !== null;

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-sm font-medium text-text-secondary">
          Testvragen ({questions.length})
        </h2>
        {!isEditing && (
          <button
            onClick={startCreate}
            className="px-4 py-2 text-xs font-medium text-white bg-accent rounded-lg hover:bg-accent/90 cursor-pointer"
          >
            Nieuwe vraag
          </button>
        )}
      </div>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-xs text-red-700">
          {error}
        </div>
      )}

      {/* Create/Edit form */}
      {isEditing && (
        <div className="mb-6 bg-surface border border-border rounded-lg p-4 space-y-4">
          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">
              Vraag *
            </label>
            <textarea
              value={form.question}
              onChange={(e) => setForm((f) => ({ ...f, question: e.target.value }))}
              rows={2}
              className="w-full px-3 py-2 text-xs bg-page-bg border border-border rounded-lg focus:outline-none focus:ring-1 focus:ring-accent resize-none"
              placeholder="Stel hier de testvraag..."
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">
              Verwachte categorie
            </label>
            <input
              type="text"
              value={form.expected_category || ""}
              onChange={(e) =>
                setForm((f) => ({
                  ...f,
                  expected_category: e.target.value || null,
                }))
              }
              className="w-full px-3 py-2 text-xs bg-page-bg border border-border rounded-lg focus:outline-none focus:ring-1 focus:ring-accent"
              placeholder="bijv. hypotheek, overlijden, arbeidsongeschiktheid"
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">
              Verwachte kernpunten
            </label>
            <div className="space-y-2">
              {form.expected_key_points.map((kp, i) => (
                <div key={i} className="flex gap-2">
                  <input
                    type="text"
                    value={kp}
                    onChange={(e) => updateKeyPoint(i, e.target.value)}
                    className="flex-1 px-3 py-2 text-xs bg-page-bg border border-border rounded-lg focus:outline-none focus:ring-1 focus:ring-accent"
                    placeholder="Kernpunt..."
                  />
                  <button
                    onClick={() => removeKeyPoint(i)}
                    className="px-2 text-xs text-text-tertiary hover:text-red-600 cursor-pointer"
                  >
                    &times;
                  </button>
                </div>
              ))}
              <button
                onClick={addKeyPoint}
                className="text-xs text-accent hover:underline cursor-pointer"
              >
                + Kernpunt toevoegen
              </button>
            </div>
          </div>

          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">
              Beschrijving
            </label>
            <input
              type="text"
              value={form.description || ""}
              onChange={(e) =>
                setForm((f) => ({ ...f, description: e.target.value || null }))
              }
              className="w-full px-3 py-2 text-xs bg-page-bg border border-border rounded-lg focus:outline-none focus:ring-1 focus:ring-accent"
              placeholder="Optionele toelichting"
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">
              Punten
            </label>
            <input
              type="number"
              min={1}
              max={100}
              value={form.points}
              onChange={(e) =>
                setForm((f) => ({ ...f, points: Math.max(1, parseInt(e.target.value) || 1) }))
              }
              className="w-24 px-3 py-2 text-xs bg-page-bg border border-border rounded-lg focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>

          <div className="flex gap-2 pt-2">
            <button
              onClick={handleSave}
              disabled={saving}
              className="px-4 py-2 text-xs font-medium text-white bg-accent rounded-lg hover:bg-accent/90 disabled:opacity-50 cursor-pointer"
            >
              {saving ? "Opslaan..." : creating ? "Aanmaken" : "Opslaan"}
            </button>
            <button
              onClick={cancel}
              className="px-4 py-2 text-xs font-medium text-text-secondary bg-page-bg border border-border rounded-lg hover:bg-surface cursor-pointer"
            >
              Annuleren
            </button>
          </div>
        </div>
      )}

      {/* Questions table */}
      {questions.length === 0 ? (
        <div className="text-center py-12 text-xs text-text-tertiary">
          Nog geen testvragen. Klik op "Nieuwe vraag" om te beginnen.
        </div>
      ) : (
        <div className="bg-surface border border-border rounded-lg overflow-hidden">
          <table className="w-full">
            <thead>
              <tr className="border-b border-border-light">
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary w-8">
                  #
                </th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">
                  Vraag
                </th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary w-32">
                  Categorie
                </th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary w-16">
                  Punten
                </th>
                <th className="text-right px-4 py-3 text-xs font-medium text-text-tertiary w-24">
                  Acties
                </th>
              </tr>
            </thead>
            <tbody>
              {questions.map((q, i) => (
                <tr
                  key={q.id}
                  className="border-b border-border-light last:border-0"
                >
                  <td className="px-4 py-3 text-xs text-text-tertiary align-top">
                    {i + 1}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-primary align-top">
                    <div>{q.question}</div>
                    {q.description && (
                      <div className="text-text-tertiary mt-0.5">
                        {q.description}
                      </div>
                    )}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary align-top">
                    {q.expected_category || "-"}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary align-top">
                    {q.points}
                  </td>
                  <td className="px-4 py-3 text-xs text-right align-top">
                    {deleteConfirm === q.id ? (
                      <span className="flex items-center justify-end gap-2">
                        <button
                          onClick={() => handleDelete(q.id)}
                          className="text-red-600 hover:underline cursor-pointer"
                        >
                          Bevestig
                        </button>
                        <button
                          onClick={() => setDeleteConfirm(null)}
                          className="text-text-tertiary hover:underline cursor-pointer"
                        >
                          Annuleer
                        </button>
                      </span>
                    ) : (
                      <span className="flex items-center justify-end gap-3">
                        <button
                          onClick={() => startEdit(q)}
                          className="text-accent hover:underline cursor-pointer"
                        >
                          Bewerk
                        </button>
                        <button
                          onClick={() => setDeleteConfirm(q.id)}
                          className="text-text-tertiary hover:text-red-600 cursor-pointer"
                        >
                          Verwijder
                        </button>
                      </span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
