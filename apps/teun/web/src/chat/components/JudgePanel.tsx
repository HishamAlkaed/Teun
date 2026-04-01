import { useState } from "react";
import type { JudgeResult, SourceVerdict } from "../lib/types";
import { useSettings } from "../hooks/useSettings";

interface JudgePanelProps {
  result: JudgeResult;
}

type Tier = "pass" | "warning" | "fail" | "unknown";

const tierConfig = {
  pass: {
    label: "Betrouwbaar",
    color: "text-green-600 bg-green-50 border-green-200",
  },
  warning: {
    label: "Beperkte betrouwbaarheid",
    color: "text-amber-600 bg-amber-50 border-amber-200",
  },
  fail: {
    label: "Onbetrouwbaar",
    color: "text-red-600 bg-red-50 border-red-200",
  },
  unknown: {
    label: "Kwaliteit onbekend",
    color: "text-text-secondary bg-surface border-border",
  },
};

export function JudgePanel({ result }: JudgePanelProps) {
  const [showDetails, setShowDetails] = useState(false);
  const { settings } = useSettings();

  const tier: Tier =
    result.score === undefined
      ? "unknown"
      : result.score >= settings.judgeThreshold
        ? "pass"
        : result.score >= settings.judgeLowThreshold
          ? "warning"
          : "fail";
  const cfg = tierConfig[tier];

  return (
    <div>
      <div className="flex items-center justify-between">
        <span
          className={`inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-xs font-medium ${cfg.color}`}
        >
          {result.score !== undefined && (
            <span className="font-mono font-bold">{result.score}</span>
          )}
          <span>{cfg.label}</span>
        </span>
        <button
          onClick={() => setShowDetails(!showDetails)}
          className="text-xs text-text-tertiary hover:text-text-secondary transition-colors"
        >
          {showDetails ? "Verberg details" : "Toon details"}
        </button>
      </div>

      {tier === "warning" && (
        <div className="mt-2 p-2 rounded-md bg-amber-50 border border-amber-100 text-xs text-amber-700">
          Dit antwoord heeft een beperkte betrouwbaarheidsscore. Extra verificatie van bronnen vereist.
        </div>
      )}

      {tier === "fail" && (
        <div className="mt-2 p-2 rounded-md bg-red-50 border border-red-100 text-xs text-red-700">
          Dit antwoord scoort onder de betrouwbaarheidsdrempel ({settings.judgeLowThreshold}).
          Raadpleeg team Acceptatie.
        </div>
      )}

      {showDetails && (
        <div className="mt-3 space-y-2">
          <div className="text-xs text-text-secondary">
            Bronverificatie: {result.sources_verified}/{result.sources_total}{" "}
            bronnen geverifieerd
          </div>

          {result.source_verdicts.map((v, i) => (
            <SourceVerdictRow key={i} verdict={v} />
          ))}

          {result.reasoning && (
            <div className="mt-2 p-2 rounded-md bg-surface border border-border text-xs text-text-secondary leading-relaxed">
              <div className="font-medium text-text-tertiary uppercase tracking-wider mb-1 text-[10px]">
                Onderbouwing
              </div>
              {result.reasoning}
            </div>
          )}

          {result.llm_skipped && (
            <div className="text-xs text-text-tertiary">
              LLM-beoordeling niet beschikbaar (geen API-sleutel geconfigureerd).
            </div>
          )}

          {result.llm_error && (
            <div className="text-xs text-text-tertiary">
              Kwaliteitscontrole gedeeltelijk niet beschikbaar: {result.llm_error}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

const statusConfig: Record<
  string,
  { icon: string; color: string }
> = {
  ok: { icon: "\u2713", color: "text-green-600" },
  document_not_found: { icon: "\u2717", color: "text-red-600" },
  line_range_out_of_bounds: { icon: "!", color: "text-amber-600" },
  quote_mismatch: { icon: "~", color: "text-amber-600" },
};

function SourceVerdictRow({ verdict }: { verdict: SourceVerdict }) {
  const cfg = statusConfig[verdict.status] ?? statusConfig.ok;
  return (
    <div className="flex items-start gap-2 text-xs">
      <span className={`font-mono font-bold ${cfg.color}`}>{cfg.icon}</span>
      <div>
        <span className="font-medium">{verdict.document}</span>
        {verdict.line_range && (
          <span className="text-text-tertiary ml-1 font-mono">
            r. {verdict.line_range}
          </span>
        )}
        {verdict.detail && (
          <div className="text-text-tertiary">{verdict.detail}</div>
        )}
      </div>
    </div>
  );
}
