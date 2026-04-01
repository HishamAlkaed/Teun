import { useCallback, useMemo, useRef, useState } from "react";
import type { VoiceProvider } from "../lib/voice/provider";
import { BrowserSTT } from "../lib/voice/browser-stt";
import { ServerSTT } from "../lib/voice/server-stt";

export function useVoiceInput() {
  const [isListening, setIsListening] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const providerRef = useRef<VoiceProvider | null>(null);

  const isSupported = useMemo(() => {
    const browser = new BrowserSTT();
    const server = new ServerSTT();
    return browser.isSupported() || server.isSupported();
  }, []);

  const getProvider = useCallback((): VoiceProvider => {
    // Prefer browser STT, fall back to server STT
    const browser = new BrowserSTT();
    if (browser.isSupported()) return browser;

    const server = new ServerSTT();
    if (server.isSupported()) return server;

    throw new Error("No voice input method available");
  }, []);

  const toggle = useCallback(async (): Promise<string | null> => {
    setError(null);

    if (isListening && providerRef.current) {
      // Stop recording
      try {
        setIsListening(false);
        const transcript = await providerRef.current.stop();
        providerRef.current = null;
        return transcript;
      } catch (err) {
        setError(err instanceof Error ? err.message : "Voice input failed");
        providerRef.current = null;
        return null;
      }
    } else {
      // Start recording
      try {
        const provider = getProvider();
        providerRef.current = provider;
        await provider.start();
        setIsListening(true);
        return null; // Transcript comes on stop
      } catch (err) {
        setError(err instanceof Error ? err.message : "Could not start voice input");
        return null;
      }
    }
  }, [isListening, getProvider]);

  return { isListening, isSupported, error, toggle };
}
