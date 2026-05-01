import { useState, useEffect } from "react";
import { Check, X, Download, Trash2, Terminal, MousePointer, Cog, Zap, GitBranch, BookOpen } from "lucide-react";
import { useClients } from "../hooks/useClients";
import { api } from "../lib/tauri";
import { Button } from "../components/ui/Button";
import { Spinner } from "../components/ui/Spinner";

const clientIcons: Record<string, React.ReactNode> = {
  "claude-code": <Terminal className="w-5 h-5" />,
  cursor: <MousePointer className="w-5 h-5" />,
  openclaw: <Cog className="w-5 h-5" />,
};

interface IntegrationItem {
  id: string;
  name: string;
  description: string;
  icon: React.ReactNode;
  installed: boolean;
  detected?: boolean;
  configPath?: string;
  onInstall: () => Promise<void>;
  onUninstall: () => Promise<void>;
}

export function Integrations() {
  const { clients, loading, refresh } = useClients();
  const [acting, setActing] = useState<string | null>(null);
  const [hookEnabled, setHookEnabled] = useState(false);
  const [gitHookEnabled, setGitHookEnabled] = useState(false);
  const [skillInstalled, setSkillInstalled] = useState(false);

  useEffect(() => {
    api.getHookStatus().then(setHookEnabled);
    api.getGitHookStatus().then(setGitHookEnabled);
    api.getSkillStatus().then(setSkillInstalled);
  }, []);

  const act = async (id: string, fn: () => Promise<void>) => {
    setActing(id);
    try {
      await fn();
    } catch (e) {
      console.error(e);
    }
    setActing(null);
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Spinner size={24} />
      </div>
    );
  }

  const items: IntegrationItem[] = [
    ...clients.map((client) => ({
      id: client.id,
      name: client.name,
      description: "MCP server — gives access to code graph, deps, impact, memory",
      icon: clientIcons[client.id] || <Cog className="w-5 h-5" />,
      installed: client.mcp_configured,
      detected: client.detected,
      configPath: client.config_path || undefined,
      onInstall: async () => { await api.installMcp(client.id); refresh(); },
      onUninstall: async () => { await api.uninstallMcp(client.id); refresh(); },
    })),
    {
      id: "hook-claude",
      name: "Search Hook",
      description: "Redirects grep/find to graphmind for structural code search",
      icon: <Zap className="w-5 h-5" />,
      installed: hookEnabled,
      detected: true,
      configPath: "~/.claude/hooks/graphmind-search.sh",
      onInstall: async () => { await api.installClaudeHook(); setHookEnabled(true); },
      onUninstall: async () => { await api.uninstallClaudeHook(); setHookEnabled(false); },
    },
    {
      id: "hook-git",
      name: "Git Hooks",
      description: "Auto-rebuild graph on commit, impact check on push",
      icon: <GitBranch className="w-5 h-5" />,
      installed: gitHookEnabled,
      detected: true,
      configPath: ".git/hooks/post-commit",
      onInstall: async () => { await api.installGitHook(); setGitHookEnabled(true); },
      onUninstall: async () => { await api.uninstallGitHook(); setGitHookEnabled(false); },
    },
    {
      id: "skill",
      name: "Claude Code Skill",
      description: "Teaches Claude the 3-layer rule: graph → memory → files",
      icon: <BookOpen className="w-5 h-5" />,
      installed: skillInstalled,
      detected: true,
      configPath: "~/.claude/skills/graphmind/SKILL.md",
      onInstall: async () => { await api.installSkill(); setSkillInstalled(true); },
      onUninstall: async () => {},
    },
  ];

  return (
    <div className="p-6 space-y-6">
      <header>
        <h1 className="text-lg font-semibold text-text-primary">Integrations</h1>
        <p className="text-sm text-text-secondary mt-0.5">
          Connect GraphMind to your AI coding tools and development workflow.
        </p>
      </header>

      <div className="grid gap-3">
        {items.map((item) => (
          <IntegrationCard
            key={item.id}
            item={item}
            isActing={acting === item.id}
            onInstall={() => act(item.id, item.onInstall)}
            onUninstall={() => act(item.id, item.onUninstall)}
          />
        ))}
      </div>
    </div>
  );
}

function IntegrationCard({
  item,
  isActing,
  onInstall,
  onUninstall,
}: {
  item: IntegrationItem;
  isActing: boolean;
  onInstall: () => void;
  onUninstall: () => void;
}) {
  return (
    <div className="bg-bg-card border border-border rounded-lg p-4 hover:border-border-hover transition-colors duration-150">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-bg-primary flex items-center justify-center text-text-secondary">
            {item.icon}
          </div>
          <div>
            <h3 className="font-medium text-text-primary">{item.name}</h3>
            <p className="text-xs text-text-muted mt-0.5">{item.description}</p>
            <div className="flex items-center gap-2 mt-1">
              {item.detected !== undefined && (
                item.detected ? (
                  <span className="flex items-center gap-1 text-xs text-success">
                    <Check className="w-3 h-3" /> Available
                  </span>
                ) : (
                  <span className="flex items-center gap-1 text-xs text-text-muted">
                    <X className="w-3 h-3" /> Not found
                  </span>
                )
              )}
              {item.installed && (
                <span className="flex items-center gap-1 text-xs text-accent">
                  <Check className="w-3 h-3" /> Installed
                </span>
              )}
            </div>
          </div>
        </div>

        <div>
          {isActing ? (
            <Spinner size={16} />
          ) : item.installed ? (
            <Button variant="danger" size="sm" onClick={onUninstall}>
              <Trash2 className="w-3.5 h-3.5" />
              Remove
            </Button>
          ) : (
            <Button
              size="sm"
              onClick={onInstall}
              disabled={item.detected === false}
            >
              <Download className="w-3.5 h-3.5" />
              Install
            </Button>
          )}
        </div>
      </div>

      {item.configPath && (
        <p className="text-[11px] text-text-muted mt-2.5 font-mono truncate">
          {item.configPath}
        </p>
      )}
    </div>
  );
}
