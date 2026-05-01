import { useState } from "react";
import { api } from "../lib/tauri";
import { Download, AlertCircle, Terminal } from "lucide-react";
import logo from "../assets/logo.png";

interface SetupProps {
  onComplete: () => void;
}

export function Setup({ onComplete }: SetupProps) {
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleInstall = async () => {
    setInstalling(true);
    setError(null);
    try {
      const result = await api.installCli();
      if (result.installed) {
        onComplete();
      } else {
        setError("Installation failed. Please try again.");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setInstalling(false);
    }
  };

  return (
    <div className="flex h-screen w-screen items-center justify-center bg-bg-primary">
      <div className="w-full max-w-md space-y-8 px-6">
        <div className="flex flex-col items-center gap-4">
          <img src={logo} alt="GraphMind" className="w-16 h-16 rounded-xl" />
          <h1 className="text-2xl font-semibold text-text-primary">Welcome to GraphMind</h1>
          <p className="text-sm text-text-muted text-center max-w-sm">
            GraphMind needs the CLI engine to index your code and serve MCP tools. Install it now to get started.
          </p>
        </div>

        <div className="bg-bg-card border border-border rounded-lg p-5 space-y-4">
          <div className="flex items-start gap-3">
            <Terminal className="w-5 h-5 text-accent mt-0.5 shrink-0" />
            <div className="space-y-1">
              <p className="text-sm font-medium text-text-primary">Install CLI Engine</p>
              <p className="text-xs text-text-muted">
                Downloads the latest <code className="bg-bg-primary px-1 py-0.5 rounded text-text-secondary">graphmind</code> binary (~10 MB) to <code className="bg-bg-primary px-1 py-0.5 rounded text-text-secondary">~/.graphmind/bin/</code>
              </p>
            </div>
          </div>

          <button
            onClick={handleInstall}
            disabled={installing}
            className="w-full flex items-center justify-center gap-2 px-4 py-2.5 text-sm font-medium bg-accent text-white rounded-md hover:bg-accent/90 disabled:opacity-50 transition-colors"
          >
            {installing ? (
              <>
                <Download className="w-4 h-4 animate-bounce" />
                Installing...
              </>
            ) : (
              <>
                <Download className="w-4 h-4" />
                Install
              </>
            )}
          </button>

          {error && (
            <div className="flex items-start gap-2 text-xs text-red-400">
              <AlertCircle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
              <span>{error}</span>
            </div>
          )}
        </div>

        <div className="text-center">
          <button
            onClick={onComplete}
            className="text-xs text-text-muted hover:text-text-secondary transition-colors"
          >
            I already have it installed via Homebrew — skip
          </button>
        </div>
      </div>
    </div>
  );
}
