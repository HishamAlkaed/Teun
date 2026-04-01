import { useEffect, useState } from "react";
import { type EvalRun, getEvalRun, stopEval } from "../lib/api";
import { StatsCard } from "./StatsCard";

interface EvalRunDetailProps {
  runId: string;
  onBack: () => void;
}

export function EvalRunDetail({ runId, onBack }: EvalRunDetailProps) {
  const [run, setRun] = useState<EvalRun | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [stopping, setStopping] = useState(false);

  useEffect(() => {
    const fetchRun = () => void getEvalRun(runId).then(setRun).catch(() => {});
    fetchRun();

    const timer = setInterval(() => {
      setRun((prev) => {
        if (prev && (prev.status === "completed" || prev.status === "failed" || prev.status === "stopped")) {
          return prev; // stop polling
        }
        fetchRun();
        return prev;
      });
    }, 4000);

    return () => clearInterval(timer);
  }, [runId]);

  const handleStop = async () => {
    setStopping(true);
    try {
      await stopEval(runId);
      const updated = await getEvalRun(runId);
      setRun(updated);
    } catch {
      // ignore — will pick up on next poll
    } finally {
      setStopping(false);
    }
  };

  if (!run) {
    return <div className="text-xs text-text-tertiary">Laden...</div>;
  }

  const isRunning = run.status === "running" || run.status === "pending";
  const progress = run.total_questions > 0
    ? (run.completed_questions / run.total_questions) * 100
    : 0;

  return (
    <div>
      <button
        onClick={onBack}
        className="mb-4 text-xs text-accent hover:underline cursor-pointer"
      >
        &larr; Terug naar overzicht
      </button>

      {/* Progress bar */}
      {isRunning && (
        <div className="mb-6">
          <div className="flex justify-between items-center mb-1">
            <span className="text-xs text-text-secondary">
              {run.completed_questions} / {run.total_questions} vragen verwerkt
            </span>
            <div className="flex items-center gap-3">
              <span className="text-xs text-text-tertiary">{progress.toFixed(0)}%</span>
              <button
                onClick={handleStop}
                disabled={stopping}
                className="px-3 py-1 text-xs font-medium text-red-700 bg-red-100 rounded hover:bg-red-200 disabled:opacity-50 cursor-pointer transition-colors"
              >
                {stopping ? "Stoppen..." : "Stop"}
              </button>
            </div>
          </div>
          <div className="w-full h-2 bg-border-light rounded-full overflow-hidden">
            <div
              className="h-full bg-accent rounded-full transition-all duration-500"
              style={{ width: `${progress}%` }}
            />
          </div>
        </div>
      )}

      {run.status === "stopped" && (
        <div className="mb-4 p-3 bg-amber-50 border border-amber-200 rounded-lg text-xs text-amber-700">
          Evaluatie gestopt na {run.completed_questions} van {run.total_questions} vragen
        </div>
      )}

      {/* Summary cards */}
      {run.summary && (
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mb-6">
          <StatsCard label="Pass" value={run.summary.pass_count} color="green" />
          <StatsCard label="Partial" value={run.summary.partial_count} color="yellow" />
          <StatsCard label="Fail" value={run.summary.fail_count} color="red" />
          <StatsCard
            label="Gem. score"
            value={run.summary.average_score.toFixed(1)}
            subtext={`${run.summary.pass_rate.toFixed(0)}% pass rate`}
          />
        </div>
      )}

      {run.error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg text-xs text-red-700">
          {run.error}
        </div>
      )}

      {/* Results table */}
      {run.results.length > 0 && (
        <div className="bg-surface border border-border rounded-lg overflow-hidden">
          <table className="w-full">
            <thead>
              <tr className="border-b border-border-light">
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary w-8">#</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Vraag</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary w-24">Score</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary w-24">Verdict</th>
              </tr>
            </thead>
            <tbody>
              {run.results.map((result, i) => (
                <tr key={result.question_id} className="border-b border-border-light last:border-0">
                  <td className="px-4 py-3 text-xs text-text-tertiary align-top">{i + 1}</td>
                  <td className="px-4 py-3 align-top">
                    <button
                      onClick={() =>
                        setExpandedId(expandedId === result.question_id ? null : result.question_id)
                      }
                      className="text-left text-xs text-text-primary hover:text-accent cursor-pointer"
                    >
                      {result.question}
                    </button>
                    {expandedId === result.question_id && (
                      <div className="mt-3 space-y-3">
                        {result.error && (
                          <div className="text-xs text-red-600">Fout: {result.error}</div>
                        )}
                        {result.actual_answer && (
                          <div>
                            <div className="text-xs font-medium text-text-secondary mb-1">Antwoord:</div>
                            <div className="text-xs text-text-secondary bg-page-bg p-2 rounded">
                              {result.actual_answer}
                            </div>
                          </div>
                        )}
                        {result.actual_rationale && (
                          <div>
                            <div className="text-xs font-medium text-text-secondary mb-1">Onderbouwing:</div>
                            <div className="text-xs text-text-secondary bg-page-bg p-2 rounded">
                              {result.actual_rationale}
                            </div>
                          </div>
                        )}
                        {result.judge_reasoning && (
                          <div>
                            <div className="text-xs font-medium text-text-secondary mb-1">Beoordeling:</div>
                            <div className="text-xs text-text-secondary bg-page-bg p-2 rounded">
                              {result.judge_reasoning}
                            </div>
                          </div>
                        )}
                        <div className="flex gap-4 text-xs text-text-tertiary">
                          {result.actual_category && (
                            <span>Categorie: {result.actual_category}</span>
                          )}
                          {result.expected_category && (
                            <span>Verwacht: {result.expected_category}</span>
                          )}
                        </div>
                      </div>
                    )}
                  </td>
                  <td className="px-4 py-3 text-xs text-text-secondary align-top">
                    {result.judge_score ?? "-"}
                  </td>
                  <td className="px-4 py-3 align-top">
                    <VerdictBadge verdict={result.judge_verdict} error={result.error} />
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

function VerdictBadge({ verdict, error }: { verdict: string | null; error: string | null }) {
  if (error) {
    return (
      <span className="inline-block px-2 py-0.5 rounded text-xs font-medium bg-red-100 text-red-700">
        Fout
      </span>
    );
  }
  if (!verdict) return <span className="text-xs text-text-tertiary">-</span>;

  const styles: Record<string, string> = {
    pass: "bg-emerald-100 text-emerald-700",
    partial: "bg-amber-100 text-amber-700",
    fail: "bg-red-100 text-red-700",
  };
  const labels: Record<string, string> = {
    pass: "Pass",
    partial: "Partial",
    fail: "Fail",
  };

  return (
    <span className={`inline-block px-2 py-0.5 rounded text-xs font-medium ${styles[verdict] || "bg-gray-100 text-gray-600"}`}>
      {labels[verdict] || verdict}
    </span>
  );
}
