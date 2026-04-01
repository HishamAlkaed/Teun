interface UnavailablePageProps {
  onRetry: () => void;
}

export function UnavailablePage({ onRetry }: UnavailablePageProps) {
  return (
    <div className="min-h-screen bg-page-bg flex items-center justify-center">
      <div className="max-w-md text-center space-y-4">
        <div className="text-4xl text-text-tertiary">~</div>
        <h1 className="text-lg font-semibold text-text-primary">
          Service tijdelijk niet beschikbaar
        </h1>
        <p className="text-sm text-text-secondary leading-relaxed">
          De AI Assistent is momenteel niet bereikbaar. Dit kan komen door
          onderhoud of een tijdelijke storing. Probeer het over enkele
          minuten opnieuw.
        </p>
        <p className="text-sm text-text-secondary">
          Neem bij aanhoudende problemen contact op met team Acceptatie.
        </p>
        <button
          onClick={onRetry}
          className="inline-flex items-center gap-2 rounded-md border border-border bg-surface px-4 py-2 text-sm font-medium text-text-primary hover:bg-page-bg transition-colors"
        >
          Opnieuw proberen
        </button>
      </div>
    </div>
  );
}
