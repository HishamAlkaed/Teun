interface VoiceButtonProps {
  isListening: boolean;
  isSupported: boolean;
  onClick: () => void;
}

export function VoiceButton({ isListening, isSupported, onClick }: VoiceButtonProps) {
  if (!isSupported) return null;

  return (
    <button
      type="button"
      onClick={onClick}
      className={`p-1.5 rounded-lg transition-colors ${
        isListening
          ? "bg-red-50 text-red-500 animate-pulse"
          : "text-text-tertiary hover:text-text-secondary hover:bg-border-light"
      }`}
      title={isListening ? "Stop opname" : "Start spraakherkenning"}
    >
      <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M12 18.75a6 6 0 0 0 6-6v-1.5m-6 7.5a6 6 0 0 1-6-6v-1.5m6 7.5v3.75m-3.75 0h7.5M12 15.75a3 3 0 0 1-3-3V4.5a3 3 0 1 1 6 0v8.25a3 3 0 0 1-3 3Z"
        />
      </svg>
    </button>
  );
}
