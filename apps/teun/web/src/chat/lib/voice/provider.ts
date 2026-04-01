export interface VoiceProvider {
  start(): Promise<void>;
  stop(): Promise<string>;
  isSupported(): boolean;
  isListening(): boolean;
}
