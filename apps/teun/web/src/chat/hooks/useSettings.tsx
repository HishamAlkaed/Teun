import { createContext, useContext, useState, useCallback, type ReactNode } from "react";

export type ChatMode = "tools" | "inline";
export type UILanguage = "nl" | "en";
export type SearchDepth = "quick" | "extensive";

export interface AppSettings {
  judgeThreshold: number;
  judgeLowThreshold: number;
  chatMode: ChatMode;
  searchDepth: SearchDepth;
  language: UILanguage;
}

const STORAGE_KEY = "teun-settings";

const defaults: AppSettings = {
  judgeThreshold: 70,
  judgeLowThreshold: 40,
  chatMode: "tools",
  searchDepth: "quick",
  language: "nl",
};

function loadSettings(): AppSettings {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored) as Partial<AppSettings>;
      return { ...defaults, ...parsed };
    }
  } catch {
    /* ignore */
  }
  return defaults;
}

function saveSettings(settings: AppSettings): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}

interface SettingsContextValue {
  settings: AppSettings;
  updateSettings: (partial: Partial<AppSettings>) => void;
}

const SettingsContext = createContext<SettingsContextValue | null>(null);

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<AppSettings>(loadSettings);

  const updateSettings = useCallback((partial: Partial<AppSettings>) => {
    setSettings((prev) => {
      const next = { ...prev, ...partial };
      saveSettings(next);
      return next;
    });
  }, []);

  return (
    <SettingsContext value={{ settings, updateSettings }}>
      {children}
    </SettingsContext>
  );
}

// eslint-disable-next-line react-refresh/only-export-components -- hook must co-locate with provider
export function useSettings(): SettingsContextValue {
  const ctx = useContext(SettingsContext);
  if (!ctx) throw new Error("useSettings must be used within SettingsProvider");
  return ctx;
}
