import { useState } from "react";
import { Check, X, Download, Trash2, Terminal, MousePointer, Cog } from "lucide-react";
import { useClients } from "../hooks/useClients";
import { api, AiClient } from "../lib/tauri";
import { Button } from "../components/ui/Button";
import { Spinner } from "../components/ui/Spinner";

const clientIcons: Record<string, React.ReactNode> = {
  "claude-code": <Terminal className="w-5 h-5" />,
  cursor: <MousePointer className="w-5 h-5" />,
  openclaw: <Cog className="w-5 h-5" />,
};

export function Integrations() {
  const { clients, loading, refresh } = useClients();
  const [acting, setActing] = useState<string | null>(null);

  const handleInstall = async (clientId: string) => {
    setActing(clientId);
    try {
      await api.installMcp(clientId);
      refresh();
    } finally {
      setActing(null);
    }
  };

  const handleUninstall = async (clientId: string) => {
    setActing(clientId);
    try {
      await api.uninstallMcp(clientId);
      refresh();
    } finally {
      setActing(null);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Spinner size={24} />
      </div>
    );
  }

  return (
    <div className="p-6 space-y-6">
      <header>
        <h1 className="text-lg font-semibold text-text-primary">AI Integrations</h1>
        <p className="text-sm text-text-secondary mt-0.5">
          Install GraphMind MCP server for your AI coding tools.
        </p>
      </header>

      <div className="grid gap-3">
        {clients.map((client) => (
          <ClientCard
            key={client.id}
            client={client}
            isActing={acting === client.id}
            onInstall={() => handleInstall(client.id)}
            onUninstall={() => handleUninstall(client.id)}
          />
        ))}
      </div>

      <div className="mt-6 p-4 bg-bg-card border border-border rounded-lg">
        <h3 className="text-sm font-medium text-text-primary">How it works</h3>
        <p className="text-xs text-text-secondary mt-1.5 leading-relaxed">
          GraphMind exposes a MCP (Model Context Protocol) server that gives AI tools
          access to your code graph — symbols, dependencies, impact analysis, and memory.
          Installing writes the server configuration to each tool's config file.
        </p>
      </div>
    </div>
  );
}

function ClientCard({
  client,
  isActing,
  onInstall,
  onUninstall,
}: {
  client: AiClient;
  isActing: boolean;
  onInstall: () => void;
  onUninstall: () => void;
}) {
  return (
    <div className="bg-bg-card border border-border rounded-lg p-4 hover:border-border-hover transition-colors duration-150">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-bg-primary flex items-center justify-center text-text-secondary">
            {clientIcons[client.id] || <Cog className="w-5 h-5" />}
          </div>
          <div>
            <h3 className="font-medium text-text-primary">{client.name}</h3>
            <div className="flex items-center gap-2 mt-0.5">
              {client.detected ? (
                <span className="flex items-center gap-1 text-xs text-success">
                  <Check className="w-3 h-3" /> Detected
                </span>
              ) : (
                <span className="flex items-center gap-1 text-xs text-text-muted">
                  <X className="w-3 h-3" /> Not found
                </span>
              )}
              {client.mcp_configured && (
                <span className="flex items-center gap-1 text-xs text-accent">
                  <Check className="w-3 h-3" /> MCP configured
                </span>
              )}
            </div>
          </div>
        </div>

        <div>
          {isActing ? (
            <Spinner size={16} />
          ) : client.mcp_configured ? (
            <Button variant="danger" size="sm" onClick={onUninstall}>
              <Trash2 className="w-3.5 h-3.5" />
              Remove
            </Button>
          ) : (
            <Button
              size="sm"
              onClick={onInstall}
              disabled={!client.detected}
            >
              <Download className="w-3.5 h-3.5" />
              Install
            </Button>
          )}
        </div>
      </div>

      {client.config_path && (
        <p className="text-[11px] text-text-muted mt-2.5 font-mono truncate">
          {client.config_path}
        </p>
      )}
    </div>
  );
}
