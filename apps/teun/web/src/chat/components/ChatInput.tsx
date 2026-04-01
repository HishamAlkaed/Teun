import { useState, useRef, useEffect } from "react";
import { useVoiceInput } from "../hooks/useVoiceInput";
import type { SearchDepth } from "../hooks/useSettings";
import { VoiceButton } from "./VoiceButton";
import { scrubMessage } from "../lib/api";
import type { ScrubResponse } from "../lib/types";

interface ChatInputProps {
  onSend: (message: string) => void;
  disabled: boolean;
  scrubEnabled?: boolean;
  onScrubSent?: () => void;
  searchDepth: SearchDepth;
  onSearchDepthChange: (depth: SearchDepth) => void;
}

const depthOptions: { value: SearchDepth; label: string }[] = [
  { value: "quick", label: "Snel zoeken" },
  { value: "extensive", label: "Uitgebreid zoeken" },
];

export function ChatInput({ onSend, disabled, scrubEnabled, onScrubSent, searchDepth, onSearchDepthChange }: ChatInputProps) {
  const [text, setText] = useState("");
  const voice = useVoiceInput();
  const [isScrubbing, setIsScrubbing] = useState(false);
  const [scrubResult, setScrubResult] = useState<ScrubResponse | null>(null);
  const [scrubError, setScrubError] = useState<string | null>(null);
  const [depthOpen, setDepthOpen] = useState(false);
  const depthRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (depthRef.current && !depthRef.current.contains(e.target as Node)) {
        setDepthOpen(false);
      }
    };
    if (depthOpen) document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [depthOpen]);

  const handleSubmit = async (e: React.SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!text.trim() || disabled || isScrubbing) return;

    // If there's a pending scrub result with PII, send the scrubbed text
    if (scrubResult) {
      onScrubSent?.();
      onSend(scrubResult.scrubbed_text);
      setText("");
      setScrubResult(null);
      return;
    }

    // If scrub is enabled, intercept and check for PII first
    if (scrubEnabled) {
      setIsScrubbing(true);
      setScrubError(null);
      try {
        const result = await scrubMessage(text.trim());
        if (result.entities.length > 0) {
          // PII found — show scrubbed text in input, wait for user to send again
          setScrubResult(result);
          setText(result.scrubbed_text);
          return;
        }
        // No PII — send immediately
        onSend(text.trim());
        setText("");
      } catch (e) {
        setScrubError(e instanceof Error ? e.message : "PII check mislukt — bericht niet verzonden");
      } finally {
        setIsScrubbing(false);
      }
      return;
    }

    // Scrub not enabled — send directly
    onSend(text.trim());
    setText("");
  };

  const handleVoice = async () => {
    const transcript = await voice.toggle();
    if (transcript) {
      setText((prev) => (prev ? `${prev} ${transcript}` : transcript));
    }
  };

  const handleCancelScrub = () => {
    setScrubResult(null);
    setScrubError(null);
  };

  const currentDepth = depthOptions.find((o) => o.value === searchDepth)!;

  return (
    <div>
      {scrubResult && scrubResult.entities.length > 0 && (
        <div className="mb-3 rounded-lg border border-amber-500/40 bg-amber-500/5 p-4 text-sm">
          <div className="flex items-start gap-2 mb-2">
            <svg className="h-4 w-4 text-amber-500 mt-0.5 shrink-0" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126ZM12 15.75h.007v.008H12v-.008Z" />
            </svg>
            <p className="font-medium text-amber-500">Persoonsgegevens gedetecteerd</p>
          </div>
          <ul className="space-y-1 mb-3 ml-6">
            {scrubResult.entities.map((e, i) => (
              <li key={i} className="text-text-secondary text-xs">
                <span className="text-text-tertiary">{e.entity_type}:</span>{" "}
                <span className="line-through text-red-400">{e.original}</span>
                {" \u2192 "}
                <span className="font-mono text-accent">{e.placeholder}</span>
              </li>
            ))}
          </ul>
          <div className="flex gap-2 justify-end">
            <button
              onClick={handleCancelScrub}
              className="px-3 py-1.5 text-xs rounded-md border border-border text-text-secondary hover:bg-surface-hover transition-colors"
            >
              Annuleren
            </button>
          </div>
        </div>
      )}

      {scrubError && (
        <div className="mb-2 text-xs text-red-500">{scrubError}</div>
      )}

      <form onSubmit={handleSubmit}>
        {voice.error && (
          <div className="mb-2 text-xs text-red-500">{voice.error}</div>
        )}
        <div className={`rounded-xl border shadow-sm transition-all ${disabled ? "border-border/60 bg-page-bg opacity-60 cursor-not-allowed" : "border-border bg-surface focus-within:border-accent focus-within:ring-1 focus-within:ring-accent/20"}`}>
          {/* Input row */}
          <div className="flex items-center gap-2 px-4 py-3">
            <input
              type="text"
              value={text}
              onChange={(e) => { setText(e.target.value); if (scrubResult) setScrubResult(null); }}
              placeholder={disabled ? "Even geduld, antwoord wordt gegenereerd..." : "Stel je vraag aan Teun..."}
              disabled={disabled}
              className="flex-1 bg-transparent text-sm text-text-primary placeholder:text-text-tertiary focus:outline-none disabled:cursor-not-allowed"
            />
            <button
              type="submit"
              disabled={disabled || !text.trim() || isScrubbing}
              className={`rounded-lg p-2 text-white disabled:opacity-30 disabled:cursor-not-allowed transition-colors ${
                scrubResult ? "bg-amber-500 hover:bg-amber-600" : "bg-accent hover:bg-accent/90"
              }`}
              title={scrubResult ? "Geschoond bericht verzenden" : undefined}
            >
              {isScrubbing ? (
                <svg className="h-4 w-4 animate-spin" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                </svg>
              ) : (
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 10.5 12 3m0 0 7.5 7.5M12 3v18" />
                </svg>
              )}
            </button>
          </div>

          {/* Toolbar row */}
          <div className="flex items-center justify-between px-4 pb-2.5">
            <div className="flex items-center gap-2">
              {scrubEnabled && (
                <span className="flex items-center gap-1 text-[11px] text-text-tertiary" title="PII bescherming actief">
                  <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M9 12.75 11.25 15 15 9.75m-3-7.036A11.959 11.959 0 0 1 3.598 6 11.99 11.99 0 0 0 3 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285Z" />
                  </svg>
                  PII
                </span>
              )}
            </div>

            <div className="flex items-center gap-2">
              {/* Search depth dropdown */}
              <div className="relative" ref={depthRef}>
                <button
                  type="button"
                  onClick={() => setDepthOpen(!depthOpen)}
                  className="flex items-center gap-1 text-[11px] text-text-secondary hover:text-text-primary transition-colors"
                >
                  {currentDepth.label}
                  <svg className={`h-3 w-3 transition-transform ${depthOpen ? "rotate-180" : ""}`} fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="m19.5 8.25-7.5 7.5-7.5-7.5" />
                  </svg>
                </button>
                {depthOpen && (
                  <div className="absolute bottom-full right-0 mb-1 bg-surface border border-border rounded-md shadow-lg py-1 min-w-[150px] z-10">
                    {depthOptions.map((opt) => (
                      <button
                        key={opt.value}
                        type="button"
                        onClick={() => { onSearchDepthChange(opt.value); setDepthOpen(false); }}
                        className={`w-full text-left px-3 py-1.5 text-xs transition-colors ${
                          searchDepth === opt.value
                            ? "text-accent font-medium bg-accent/5"
                            : "text-text-secondary hover:bg-surface-hover"
                        }`}
                      >
                        {opt.label}
                      </button>
                    ))}
                  </div>
                )}
              </div>

              <VoiceButton
                isListening={voice.isListening}
                isSupported={voice.isSupported}
                onClick={handleVoice}
              />
            </div>
          </div>
        </div>
      </form>
    </div>
  );
}
