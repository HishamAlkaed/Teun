import { useEffect, useState } from "react";
import {
  type FeedbackStats,
  type RejectedMessage,
  getFeedbackStats,
  getRecentRejections,
} from "../lib/api";
import { StatsCard } from "./StatsCard";

export function FeedbackTab() {
  const [stats, setStats] = useState<FeedbackStats | null>(null);
  const [rejections, setRejections] = useState<RejectedMessage[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([getFeedbackStats(), getRecentRejections()])
      .then(([s, r]) => {
        setStats(s);
        setRejections(r);
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  if (loading) {
    return <div className="text-xs text-text-tertiary">Laden...</div>;
  }

  return (
    <div>
      <h2 className="text-sm font-medium text-text-secondary mb-6">Feedback overzicht</h2>

      {/* Stats cards */}
      {stats && (
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mb-8">
          <StatsCard label="Totaal beoordeeld" value={stats.total} />
          <StatsCard
            label="Goedgekeurd"
            value={`${stats.approval_rate.toFixed(0)}%`}
            subtext={`${stats.approved} antwoorden`}
            color="green"
          />
          <StatsCard
            label="Deels correct"
            value={`${stats.partial_rate.toFixed(0)}%`}
            subtext={`${stats.partial} antwoorden`}
            color="yellow"
          />
          <StatsCard
            label="Afgekeurd"
            value={`${stats.rejection_rate.toFixed(0)}%`}
            subtext={`${stats.rejected} antwoorden`}
            color="red"
          />
        </div>
      )}

      {/* 7-day trend */}
      {stats && stats.trend.length > 0 && (
        <div className="mb-8">
          <h3 className="text-xs font-medium text-text-secondary mb-3">Laatste 7 dagen</h3>
          <TrendChart trend={stats.trend} />
        </div>
      )}

      {/* Recent rejections */}
      <div>
        <h3 className="text-xs font-medium text-text-secondary mb-3">
          Recente afkeuringen en deels correct
        </h3>
        {rejections.length === 0 ? (
          <div className="text-center py-8 text-xs text-text-tertiary">
            Nog geen afkeuringen
          </div>
        ) : (
          <div className="space-y-3">
            {rejections.map((r, i) => (
              <div
                key={`${r.session_id}-${r.message_id}-${i}`}
                className="bg-surface border border-border rounded-lg p-4"
              >
                <div className="flex items-start justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <span
                      className={`inline-block px-2 py-0.5 rounded text-xs font-medium ${
                        r.feedback_status === "rejected"
                          ? "bg-red-100 text-red-700"
                          : "bg-amber-100 text-amber-700"
                      }`}
                    >
                      {r.feedback_status === "rejected" ? "Afgekeurd" : "Deels correct"}
                    </span>
                    <span className="text-xs text-text-tertiary">
                      {r.session_title || "Onbekende sessie"}
                    </span>
                  </div>
                  <span className="text-xs text-text-tertiary">
                    {r.feedback_at ? formatDate(r.feedback_at) : ""}
                  </span>
                </div>
                <div className="text-xs text-text-secondary mb-2 line-clamp-2">
                  {truncate(r.answer, 200)}
                </div>
                {r.feedback_comment && (
                  <div className="text-xs text-text-primary bg-page-bg p-2 rounded">
                    {r.feedback_comment}
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function TrendChart({ trend }: { trend: { date: string; status: string; count: number }[] }) {
  // Group by date
  const dates = [...new Set(trend.map((t) => t.date))].sort();
  const byDate: Record<string, Record<string, number>> = {};
  for (const entry of trend) {
    byDate[entry.date] ??= {};
    byDate[entry.date][entry.status] = entry.count;
  }

  const maxTotal = Math.max(
    ...dates.map((d) => {
      const vals = byDate[d];
      return (vals.approved || 0) + (vals.partial || 0) + (vals.rejected || 0);
    }),
    1,
  );

  return (
    <div className="flex items-end gap-2 h-24">
      {dates.map((date) => {
        const vals = byDate[date];
        const approved = vals.approved || 0;
        const partial = vals.partial || 0;
        const rejected = vals.rejected || 0;
        const total = approved + partial + rejected;
        const heightPct = (total / maxTotal) * 100;

        return (
          <div key={date} className="flex-1 flex flex-col items-center gap-1">
            <div className="w-full flex flex-col justify-end" style={{ height: "80px" }}>
              <div
                className="w-full rounded-t overflow-hidden"
                style={{ height: `${heightPct}%` }}
              >
                {rejected > 0 && (
                  <div
                    className="bg-red-400"
                    style={{ height: `${(rejected / total) * 100}%` }}
                  />
                )}
                {partial > 0 && (
                  <div
                    className="bg-amber-400"
                    style={{ height: `${(partial / total) * 100}%` }}
                  />
                )}
                {approved > 0 && (
                  <div
                    className="bg-emerald-400"
                    style={{ height: `${(approved / total) * 100}%` }}
                  />
                )}
              </div>
            </div>
            <span className="text-[10px] text-text-tertiary">
              {date.slice(5)}
            </span>
          </div>
        );
      })}
    </div>
  );
}

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleDateString("nl-NL", {
      day: "numeric",
      month: "short",
    });
  } catch {
    return "";
  }
}

function truncate(text: string, maxLen: number): string {
  if (text.length <= maxLen) return text;
  return text.slice(0, maxLen) + "...";
}
