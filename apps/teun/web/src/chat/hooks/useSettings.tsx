import { createContext, useContext, useState, useCallback, type ReactNode } from "react";

export type ChatMode = "tools" | "inline";
export type UILanguage = "nl" | "en";
export type SearchDepth = "quick" | "extensive";

export interface AppSettings {
  judgeThreshold: number;
  judgeLowThreshold: number;
  language: UILanguage;
}

const defaults: AppSettings = {
  judgeThreshold: 70,
  judgeLowThreshold: 40,
  language: "nl",
};

interface SettingsContextValue {
  settings: AppSettings;
  updateSettings: (partial: Partial<AppSettings>) => void;
}

const SettingsContext = createContext<SettingsContextValue | null>(null);

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<AppSettings>(defaults);

  const updateSettings = useCallback((partial: Partial<AppSettings>) => {
    setSettings((prev) => ({ ...prev, ...partial }));
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
