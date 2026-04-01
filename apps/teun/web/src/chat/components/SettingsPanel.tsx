import { useEffect, useState } from "react";
import { useSettings, type ChatMode, type UILanguage } from "../hooks/useSettings";
import { getBackendSettings, updateBackendSettings } from "../lib/api";
import type { BackendSettings } from "../lib/types";

export function SettingsPanel() {
  const { settings, updateSettings } = useSettings();
  const [backend, setBackend] = useState<BackendSettings | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    getBackendSettings()
      .then(setBackend)
      .catch(() => setBackend({ scrub_enabled: false }));
  }, []);

  const saveBackend = async (partial: Partial<BackendSettings>) => {
    if (!backend) return;
    const updated = { ...backend, ...partial };
    setSaving(true);
    try {
      const result = await updateBackendSettings(updated);
      setBackend(result);
    } catch {
      /* ignore */
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="max-w-xl mx-auto py-8 space-y-8">
      <h2 className="text-lg font-semibold text-text-primary">Instellingen</h2>

      {/* Judge thresholds */}
      <div className="space-y-3">
        <label className="text-sm font-medium text-text-primary">
          Betrouwbaarheidsdrempels
        </label>
        <p className="text-xs text-text-tertiary">
          Score boven de hoge drempel: betrouwbaar (groen). Tussen de drempels:
          beperkte betrouwbaarheid (oranje). Onder de lage drempel: onbetrouwbaar (rood).
        </p>
        <div className="space-y-2">
          <div className="flex items-center gap-3">
            <span className="text-xs text-text-secondary w-10">Hoog</span>
            <input
              type="range"
              min={0}
              max={100}
              value={settings.judgeThreshold}
              onChange={(e) =>
                updateSettings({ judgeThreshold: Math.max(Number(e.target.value), settings.judgeLowThreshold) })
              }
              className="flex-1 accent-green-500"
            />
            <span className="text-sm font-mono font-medium text-text-primary w-8 text-right">
              {settings.judgeThreshold}
            </span>
          </div>
          <div className="flex items-center gap-3">
            <span className="text-xs text-text-secondary w-10">Laag</span>
            <input
              type="range"
              min={0}
              max={100}
              value={settings.judgeLowThreshold}
              onChange={(e) =>
                updateSettings({ judgeLowThreshold: Math.min(Number(e.target.value), settings.judgeThreshold) })
              }
              className="flex-1 accent-red-500"
            />
            <span className="text-sm font-mono font-medium text-text-primary w-8 text-right">
              {settings.judgeLowThreshold}
            </span>
          </div>
        </div>
      </div>

      {/* Chat mode */}
      <div className="space-y-3">
        <label className="text-sm font-medium text-text-primary">
          Chat modus
        </label>
        <div className="flex gap-2">
          {(["tools", "inline"] as ChatMode[]).map((mode) => (
            <button
              key={mode}
              onClick={() => updateSettings({ chatMode: mode })}
              className={`px-3 py-1.5 text-xs rounded-md border transition-colors ${
                settings.chatMode === mode
                  ? "bg-accent text-white border-accent"
                  : "bg-surface text-text-secondary border-border hover:border-accent/50"
              }`}
            >
              {mode === "tools" ? "Tools" : "Inline"}
            </button>
          ))}
        </div>
        <p className="text-xs text-text-tertiary">
          {settings.chatMode === "tools"
            ? "Claude doorzoekt beleidsdocumenten met Grep en Read tools. Geeft nauwkeurige bronverwijzingen met regelnummers."
            : "Alle beleidsdocumenten worden in de systeemprompt geladen. Geen tool-gebruik, snellere antwoorden."}
        </p>
      </div>

      {/* Language */}
      <div className="space-y-3">
        <label className="text-sm font-medium text-text-primary">
          Taal / Language
        </label>
        <div className="flex gap-2">
          {([
            { value: "nl", label: "Nederlands" },
            { value: "en", label: "English" },
          ] as { value: UILanguage; label: string }[]).map((lang) => (
            <button
              key={lang.value}
              onClick={() => updateSettings({ language: lang.value })}
              className={`px-3 py-1.5 text-xs rounded-md border transition-colors ${
                settings.language === lang.value
                  ? "bg-accent text-white border-accent"
                  : "bg-surface text-text-secondary border-border hover:border-accent/50"
              }`}
            >
              {lang.label}
            </button>
          ))}
        </div>
        <p className="text-xs text-text-tertiary">
          {settings.language === "nl"
            ? "De assistent communiceert in het Nederlands."
            : "The assistant communicates in English."}
        </p>
      </div>

      {/* PII Scrub */}
      {backend && (
        <div className="space-y-3 border-t border-border pt-6">
          <label className="text-sm font-medium text-text-primary">
            PII Bescherming
          </label>
          <p className="text-xs text-text-tertiary">
            Detecteer en vervang persoonsgegevens (BSN, namen, etc.) voordat berichten naar het LLM worden gestuurd.
          </p>
          <div className="flex items-center gap-3">
            <button
              onClick={() => saveBackend({ scrub_enabled: !backend.scrub_enabled })}
              disabled={saving}
              className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${
                backend.scrub_enabled ? "bg-accent" : "bg-border"
              }`}
            >
              <span
                className={`inline-block h-3.5 w-3.5 transform rounded-full bg-white transition-transform ${
                  backend.scrub_enabled ? "translate-x-4.5" : "translate-x-0.5"
                }`}
              />
            </button>
            <span className="text-xs text-text-secondary">
              {backend.scrub_enabled ? "Ingeschakeld" : "Uitgeschakeld"}
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
