import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error?: Error;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("ErrorBoundary caught:", error, info);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="min-h-screen bg-page-bg flex items-center justify-center">
          <div className="max-w-md text-center space-y-4">
            <div className="text-4xl text-text-tertiary">!</div>
            <h1 className="text-lg font-semibold text-text-primary">
              Er is iets misgegaan
            </h1>
            <p className="text-sm text-text-secondary leading-relaxed">
              De applicatie heeft een onverwachte fout ondervonden.
              Probeer de pagina te vernieuwen. Neem contact op met team
              Acceptatie als het probleem aanhoudt.
            </p>
            {this.state.error && (
              <p className="text-xs text-text-tertiary font-mono">
                {this.state.error.message}
              </p>
            )}
            <button
              onClick={() => window.location.reload()}
              className="inline-flex items-center gap-2 rounded-md border border-border bg-surface px-4 py-2 text-sm font-medium text-text-primary hover:bg-page-bg transition-colors"
            >
              Pagina vernieuwen
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
