import { useCallback, useEffect, useState } from "react";
import { type EvalRunSummary, listEvalRuns, startEval } from "../lib/api";
import { EvalRunDetail } from "./EvalRunDetail";

export function EvalTab() {
  const [runs, setRuns] = useState<EvalRunSummary[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<string>("inline");

  const loadRuns = useCallback(() => {
    void listEvalRuns().then(setRuns);
  }, []);

  useEffect(() => {
    loadRuns();
  }, [loadRuns]);

  const handleStart = async () => {
    setStarting(true);
    setError(null);
    try {
      const result = await startEval(mode);
      setSelectedRunId(result.id);
      loadRuns();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Onbekende fout");
    } finally {
      setStarting(false);
    }
  };

  if (selectedRunId) {
    return (
      <EvalRunDetail
        runId={selectedRunId}
        onBack={() => {
          setSelectedRunId(null);
          loadRuns();
        }}
      />
    );
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-sm font-medium text-text-secondary">Evaluaties</h2>
        <div className="flex items-center gap-2">
          <select
            value={mode}
            onChange={(e) => setMode(e.target.value)}
            className="px-3 py-2 text-xs border border-border rounded-lg bg-surface text-text-primary"
          >
            <option value="inline">Inline</option>
          </select>
          <button
            onClick={handleStart}
            disabled={starting}
            className="px-4 py-2 text-xs font-medium text-white bg-accent rounded-lg hover:bg-accent/90 disabled:opacity-50 cursor-pointer"
          >
            {starting ? "Starten..." : "Start evaluatie"}
          </button>
        </div>
      </div>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-xs text-red-700">
          {error}
        </div>
      )}

      {runs.length === 0 ? (
        <div className="text-center py-12 text-xs text-text-tertiary">
          Nog geen evaluaties uitgevoerd
        </div>
      ) : (
        <div className="bg-surface border border-border rounded-lg overflow-hidden">
          <table className="w-full">
            <thead>
              <tr className="border-b border-border-light">
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Datum</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Status</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Vragen</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Score</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Pass rate</th>
              </tr>
            </thead>
            <tbody>
              {runs.map((run) => (
                <tr
                  key={run.id}
                  onClick={() => setSelectedRunId(run.id)}
                  className="border-b border-border-light last:border-0 hover:bg-page-bg cursor-pointer"
                >
                  <td className="px-4 py-3 text-xs text-text-primary">
                    {formatDate(run.created_at)}
                  </td>
                  <td className="px-4 py-3">
                    <StatusBadge status={run.status} />
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary">
                    {run.completed_questions} / {run.total_questions}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary">
                    {run.summary ? run.summary.average_score.toFixed(1) : "-"}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary">
                    {run.summary ? `${run.summary.pass_rate.toFixed(0)}%` : "-"}
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

function StatusBadge({ status }: { status: string }) {
  const styles: Record<string, string> = {
    pending: "bg-gray-100 text-gray-600",
    running: "bg-blue-100 text-blue-700",
    completed: "bg-emerald-100 text-emerald-700",
    failed: "bg-red-100 text-red-700",
    stopped: "bg-amber-100 text-amber-700",
  };
  const labels: Record<string, string> = {
    pending: "Wachtend",
    running: "Bezig",
    completed: "Afgerond",
    failed: "Mislukt",
    stopped: "Gestopt",
  };

  return (
    <span className={`inline-block px-2 py-0.5 rounded text-xs font-medium ${styles[status] || styles.pending}`}>
      {labels[status] || status}
    </span>
  );
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString("nl-NL", {
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
