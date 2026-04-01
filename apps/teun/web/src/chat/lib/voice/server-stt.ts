import type { VoiceProvider } from "./provider";

/**
 * Server-side speech-to-text via audio upload to /api/transcribe.
 * Uses MediaRecorder API to capture audio, sends to backend for transcription.
 * Fallback when Web Speech API is not available.
 */
export class ServerSTT implements VoiceProvider {
  private mediaRecorder: MediaRecorder | null = null;
  private chunks: Blob[] = [];
  private listening = false;

  isSupported(): boolean {
    return (
      typeof window !== "undefined" &&
      "MediaRecorder" in window &&
      "mediaDevices" in navigator
    );
  }

  isListening(): boolean {
    return this.listening;
  }

  async start(): Promise<void> {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    this.chunks = [];
    this.mediaRecorder = new MediaRecorder(stream, {
      mimeType: "audio/webm;codecs=opus",
    });
    this.mediaRecorder.ondataavailable = (e) => {
      if (e.data.size > 0) this.chunks.push(e.data);
    };
    this.mediaRecorder.start();
    this.listening = true;
  }

  async stop(): Promise<string> {
    return new Promise((resolve, reject) => {
      if (!this.mediaRecorder) {
        this.listening = false;
        reject(new Error("Recording not started"));
        return;
      }

      this.mediaRecorder.onstop = async () => {
        this.listening = false;
        const blob = new Blob(this.chunks, { type: "audio/webm" });

        // Stop all audio tracks
        this.mediaRecorder?.stream
          .getTracks()
          .forEach((track) => track.stop());

        try {
          const formData = new FormData();
          formData.append("audio", blob, "recording.webm");

          const res = await fetch("/api/teun/transcribe", {
            method: "POST",
            body: formData,
          });

          if (!res.ok) throw new Error(`Transcription failed: ${res.status}`);

          const data = await res.json() as { text?: string };
          resolve(data.text ?? "");
        } catch (err: unknown) {
          reject(err instanceof Error ? err : new Error(String(err)));
        }
      };

      this.mediaRecorder.stop();
    });
  }
}
