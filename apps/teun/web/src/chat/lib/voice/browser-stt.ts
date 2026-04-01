import type { VoiceProvider } from "./provider";

// Web Speech API type declarations
interface SpeechRecognitionEvent {
  results: { [index: number]: { [index: number]: { transcript: string } } };
}

interface SpeechRecognitionErrorEvent {
  error: string;
}

interface SpeechRecognitionInstance {
  lang: string;
  interimResults: boolean;
  continuous: boolean;
  onresult: ((event: SpeechRecognitionEvent) => void) | null;
  onerror: ((event: SpeechRecognitionErrorEvent) => void) | null;
  onend: (() => void) | null;
  start(): void;
  stop(): void;
  abort(): void;
}

interface SpeechRecognitionConstructor {
  new (): SpeechRecognitionInstance;
}

declare global {
  interface Window {
    SpeechRecognition?: SpeechRecognitionConstructor;
    webkitSpeechRecognition?: SpeechRecognitionConstructor;
  }
}

/**
 * Browser-based speech-to-text using the Web Speech API.
 * Works in Chrome/Edge. Limited support in Firefox.
 */
export class BrowserSTT implements VoiceProvider {
  private recognition: SpeechRecognitionInstance | null = null;
  private listening = false;

  isSupported(): boolean {
    return (
      typeof window !== "undefined" &&
      ("SpeechRecognition" in window || "webkitSpeechRecognition" in window)
    );
  }

  isListening(): boolean {
    return this.listening;
  }

  start(): Promise<void> {
    if (!this.isSupported()) {
      return Promise.reject(new Error("Web Speech API is not supported in this browser"));
    }

    const Ctor = window.SpeechRecognition ?? window.webkitSpeechRecognition;
    if (!Ctor) return Promise.reject(new Error("SpeechRecognition not available"));

    this.recognition = new Ctor();
    this.recognition.lang = "nl-NL";
    this.recognition.interimResults = false;
    this.recognition.continuous = false;
    this.listening = true;
    return Promise.resolve();
  }

  async stop(): Promise<string> {
    return new Promise((resolve, reject) => {
      if (!this.recognition) {
        this.listening = false;
        reject(new Error("Recognition not started"));
        return;
      }

      this.recognition.onresult = (event) => {
        this.listening = false;
        const transcript = event.results[0][0].transcript;
        resolve(transcript);
      };

      this.recognition.onerror = (event) => {
        this.listening = false;
        reject(new Error(`Speech recognition error: ${event.error}`));
      };

      this.recognition.onend = () => {
        this.listening = false;
      };

      this.recognition.start();
    });
  }
}
