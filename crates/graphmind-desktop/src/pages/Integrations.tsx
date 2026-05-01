import { useState, useEffect } from "react";
import { Check, X, Download, Trash2, Terminal, MousePointer, Cog, Zap, GitBranch, BookOpen } from "lucide-react";
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
  const [hookEnabled, setHookEnabled] = useState(false);
  const [hookLoading, setHookLoading] = useState(false);
  const [gitHookEnabled, setGitHookEnabled] = useState(false);
  const [gitHookLoading, setGitHookLoading] = useState(false);
  const [skillInstalled, setSkillInstalled] = useState(false);
  const [skillLoading, setSkillLoading] = useState(false);

  useEffect(() => {
    api.getHookStatus().then(setHookEnabled);
    api.getGitHookStatus().then(setGitHookEnabled);
    api.getSkillStatus().then(setSkillInstalled);
  }, []);

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

  const toggleHook = async () => {
    setHookLoading(true);
    try {
      if (hookEnabled) {
        await api.uninstallClaudeHook();
        setHookEnabled(false);
      } else {
        await api.installClaudeHook();
        setHookEnabled(true);
      }
    } catch (e) {
      console.error(e);
    }
    setHookLoading(false);
  };

  const toggleGitHook = async () => {
    setGitHookLoading(true);
    try {
      if (gitHookEnabled) {
        await api.uninstallGitHook();
        setGitHookEnabled(false);
      } else {
        await api.installGitHook();
        setGitHookEnabled(true);
      }
    } catch (e) {
      console.error(e);
    }
    setGitHookLoading(false);
  };

  const handleInstallSkill = async () => {
    setSkillLoading(true);
    try {
      await api.installSkill();
      setSkillInstalled(true);
    } catch (e) {
      console.error(e);
    }
    setSkillLoading(false);
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

      {/* Claude Code Enhancements */}
      <section className="space-y-3">
        <h2 className="text-sm font-medium text-text-primary">Claude Code Enhancements</h2>

        <div className="flex items-center justify-between p-3 bg-bg-card rounded-lg border border-border">
          <div className="flex items-center gap-3">
            <Zap className={`w-4 h-4 ${hookEnabled ? "text-accent" : "text-text-muted"}`} />
            <div>
              <p className="text-sm text-text-primary">Search Hook</p>
              <p className="text-xs text-text-muted">
                Redirects grep/find to graphmind for structural code search
              </p>
            </div>
          </div>
          <button
            onClick={toggleHook}
            disabled={hookLoading}
            className={`relative w-10 h-5 rounded-full transition-colors duration-200 ${
              hookEnabled ? "bg-accent" : "bg-border"
            } ${hookLoading ? "opacity-50" : ""}`}
          >
            <span
              className={`absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full transition-transform duration-200 ${
                hookEnabled ? "translate-x-5" : ""
              }`}
            />
          </button>
        </div>

        <div className="flex items-center justify-between p-3 bg-bg-card rounded-lg border border-border">
          <div className="flex items-center gap-3">
            <GitBranch className={`w-4 h-4 ${gitHookEnabled ? "text-accent" : "text-text-muted"}`} />
            <div>
              <p className="text-sm text-text-primary">Git Hooks</p>
              <p className="text-xs text-text-muted">
                Auto-rebuild on commit, impact check on push
              </p>
            </div>
          </div>
          <button
            onClick={toggleGitHook}
            disabled={gitHookLoading}
            className={`relative w-10 h-5 rounded-full transition-colors duration-200 ${
              gitHookEnabled ? "bg-accent" : "bg-border"
            } ${gitHookLoading ? "opacity-50" : ""}`}
          >
            <span
              className={`absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full transition-transform duration-200 ${
                gitHookEnabled ? "translate-x-5" : ""
              }`}
            />
          </button>
        </div>

        <div className="flex items-center justify-between p-3 bg-bg-card rounded-lg border border-border">
          <div className="flex items-center gap-3">
            <BookOpen className={`w-4 h-4 ${skillInstalled ? "text-accent" : "text-text-muted"}`} />
            <div>
              <p className="text-sm text-text-primary">Claude Code Skill</p>
              <p className="text-xs text-text-muted">
                Teaches Claude the 3-layer rule: graph first, memory second, files last
              </p>
            </div>
          </div>
          {skillInstalled ? (
            <span className="flex items-center gap-1 text-xs text-accent">
              <Check className="w-3.5 h-3.5" />
              Installed
            </span>
          ) : (
            <button
              onClick={handleInstallSkill}
              disabled={skillLoading}
              className={`px-3 py-1.5 text-xs font-medium bg-accent text-white rounded-md hover:bg-accent/90 ${
                skillLoading ? "opacity-50" : ""
              }`}
            >
              {skillLoading ? "Installing..." : "Install"}
            </button>
          )}
        </div>
      </section>

      <div className="p-4 bg-bg-card border border-border rounded-lg">
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
