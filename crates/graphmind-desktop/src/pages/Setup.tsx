import { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { api, AiClient, EmbeddingSettingsInput } from "../lib/tauri";
import {
  Download,
  AlertCircle,
  Terminal,
  FolderOpen,
  Check,
  ChevronRight,
  Zap,
  Brain,
  Sparkles,
  GitBranch,
  BookOpen,
  MousePointer,
  Cog,
  Loader2,
} from "lucide-react";
import logo from "../assets/logo.png";

interface SetupProps {
  onComplete: () => void;
}

const TOTAL_STEPS = 5;

function StepIndicator({ current, total }: { current: number; total: number }) {
  return (
    <div className="flex items-center gap-2">
      {Array.from({ length: total }, (_, i) => (
        <div
          key={i}
          className={`h-1.5 rounded-full transition-all duration-300 ${
            i < current
              ? "w-6 bg-accent"
              : i === current
              ? "w-6 bg-accent/50"
              : "w-1.5 bg-border"
          }`}
        />
      ))}
    </div>
  );
}

function StepShell({
  children,
  step,
}: {
  children: React.ReactNode;
  step: number;
}) {
  return (
    <div className="flex h-screen w-screen items-center justify-center bg-bg-primary">
      <div className="w-full max-w-lg px-6">
        <AnimatePresence mode="wait">
          <motion.div
            key={step}
            initial={{ opacity: 0, x: 40 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -40 }}
            transition={{ duration: 0.25, ease: "easeOut" }}
            className="space-y-6"
          >
            {children}
          </motion.div>
        </AnimatePresence>
        <div className="flex justify-center mt-8">
          <StepIndicator current={step} total={TOTAL_STEPS} />
        </div>
      </div>
    </div>
  );
}

// Step 0: Welcome
function StepWelcome({ onNext }: { onNext: () => void }) {
  return (
    <StepShell step={0}>
      <div className="flex flex-col items-center gap-5">
        <img src={logo} alt="GraphMind" className="w-20 h-20 rounded-2xl" />
        <h1 className="text-2xl font-semibold text-text-primary">
          Welcome to GraphMind
        </h1>
        <p className="text-sm text-text-secondary text-center max-w-sm leading-relaxed">
          Give your AI coding tools structural understanding of your codebase.
          Local-first, no cloud, no telemetry.
        </p>
      </div>

      <div className="space-y-3">
        {[
          {
            icon: <GitBranch className="w-4 h-4" />,
            title: "Structural graph",
            desc: "Function-level code knowledge graph built from AST",
          },
          {
            icon: <Brain className="w-4 h-4" />,
            title: "Semantic search",
            desc: "Vector embeddings find code by meaning, not just text",
          },
          {
            icon: <Zap className="w-4 h-4" />,
            title: "AI integrations",
            desc: "MCP server, hooks, and skills for Claude Code & Cursor",
          },
        ].map((item) => (
          <div
            key={item.title}
            className="flex items-start gap-3 bg-bg-card border border-border rounded-lg px-4 py-3"
          >
            <div className="mt-0.5 text-accent">{item.icon}</div>
            <div>
              <p className="text-sm font-medium text-text-primary">
                {item.title}
              </p>
              <p className="text-xs text-text-muted">{item.desc}</p>
            </div>
          </div>
        ))}
      </div>

      <button
        onClick={onNext}
        className="w-full flex items-center justify-center gap-2 px-4 py-3 text-sm font-medium bg-accent text-white rounded-lg hover:bg-accent-hover transition-colors"
      >
        Get started
        <ChevronRight className="w-4 h-4" />
      </button>
    </StepShell>
  );
}

// Step 1: Install CLI
function StepInstallCli({
  onNext,
  onSkip,
}: {
  onNext: () => void;
  onSkip: () => void;
}) {
  const [status, setStatus] = useState<
    "idle" | "installing" | "done" | "error"
  >("idle");
  const [error, setError] = useState<string | null>(null);
  const [cliPath, setCliPath] = useState<string | null>(null);

  useEffect(() => {
    api.checkCliInstalled().then((s) => {
      if (s.installed) {
        setCliPath(s.path || null);
        setStatus("done");
      }
    });
  }, []);

  const handleInstall = async () => {
    setStatus("installing");
    setError(null);
    try {
      const result = await api.installCli();
      if (!result.installed) {
        setStatus("error");
        setError("Installation failed. Please try again.");
        return;
      }
      const path = await api.ensureCliInPath();
      setCliPath(path);
      setStatus("done");
    } catch (e) {
      setStatus("error");
      setError(String(e));
    }
  };

  return (
    <StepShell step={1}>
      <div className="flex flex-col items-center gap-3">
        <div className="w-12 h-12 rounded-xl bg-accent/10 flex items-center justify-center">
          <Terminal className="w-6 h-6 text-accent" />
        </div>
        <h2 className="text-xl font-semibold text-text-primary">
          Install CLI Engine
        </h2>
        <p className="text-sm text-text-muted text-center max-w-sm">
          GraphMind needs the CLI to index code, serve MCP tools, and manage
          hooks. One click — ready in seconds.
        </p>
      </div>

      <div className="bg-bg-card border border-border rounded-lg p-4 space-y-3">
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <p className="text-sm font-medium text-text-primary">
              graphmind CLI
            </p>
            <p className="text-xs text-text-muted">
              ~10 MB binary → <code className="text-text-secondary">~/.graphmind/bin/</code>
            </p>
          </div>
          {status === "done" && (
            <div className="flex items-center gap-1.5 text-success text-xs font-medium">
              <Check className="w-4 h-4" />
              Installed
            </div>
          )}
        </div>

        {status !== "done" && (
          <button
            onClick={handleInstall}
            disabled={status === "installing"}
            className="w-full flex items-center justify-center gap-2 px-4 py-2.5 text-sm font-medium bg-accent text-white rounded-md hover:bg-accent-hover disabled:opacity-50 transition-colors"
          >
            {status === "installing" ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin" />
                Installing...
              </>
            ) : (
              <>
                <Download className="w-4 h-4" />
                Install
              </>
            )}
          </button>
        )}

        {error && (
          <div className="flex items-start gap-2 text-xs text-danger">
            <AlertCircle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
            <span>{error}</span>
          </div>
        )}

        {cliPath && (
          <p className="text-[11px] text-text-muted font-mono truncate">
            {cliPath}
          </p>
        )}
      </div>

      {status === "done" ? (
        <button
          onClick={onNext}
          className="w-full flex items-center justify-center gap-2 px-4 py-3 text-sm font-medium bg-accent text-white rounded-lg hover:bg-accent-hover transition-colors"
        >
          Continue
          <ChevronRight className="w-4 h-4" />
        </button>
      ) : (
        <button
          onClick={onSkip}
          className="w-full text-center text-xs text-text-muted hover:text-text-secondary transition-colors py-2"
        >
          I already have it via Homebrew — skip
        </button>
      )}
    </StepShell>
  );
}

// Step 2: Add first project
function StepAddProject({ onNext }: { onNext: () => void }) {
  const [status, setStatus] = useState<
    "idle" | "adding" | "building" | "done" | "error"
  >("idle");
  const [projectName, setProjectName] = useState<string | null>(null);
  const [stats, setStats] = useState<{
    symbols: number;
    files: number;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleOpenFolder = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false });
      if (!selected) return;

      const path = typeof selected === "string" ? selected : selected[0];
      if (!path) return;

      setStatus("adding");
      setError(null);
      const project = await api.addProject(path);
      setProjectName(project.slug);

      setStatus("building");
      await api.buildProject(project.slug, true);

      const updated = await api.getProjectStatus(project.slug);
      setStats(
        updated.stats
          ? { symbols: updated.stats.symbols, files: updated.stats.files }
          : null
      );
      setStatus("done");
    } catch (e) {
      setStatus("error");
      setError(String(e));
    }
  };

  return (
    <StepShell step={2}>
      <div className="flex flex-col items-center gap-3">
        <div className="w-12 h-12 rounded-xl bg-accent/10 flex items-center justify-center">
          <FolderOpen className="w-6 h-6 text-accent" />
        </div>
        <h2 className="text-xl font-semibold text-text-primary">
          Add your first project
        </h2>
        <p className="text-sm text-text-muted text-center max-w-sm">
          Pick a code repository. GraphMind will parse it and build a structural
          graph of every symbol and dependency.
        </p>
      </div>

      <div className="bg-bg-card border border-border rounded-lg p-4 space-y-3">
        {status === "done" && projectName ? (
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <Check className="w-4 h-4 text-success" />
              <p className="text-sm font-medium text-text-primary">
                {projectName}
              </p>
            </div>
            {stats && (
              <p className="text-xs text-text-muted">
                Indexed {stats.symbols} symbols across {stats.files} files
              </p>
            )}
          </div>
        ) : status === "building" ? (
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <Loader2 className="w-4 h-4 text-accent animate-spin" />
              <p className="text-sm text-text-primary">
                Building graph for <span className="font-medium">{projectName}</span>...
              </p>
            </div>
            <div className="h-1.5 bg-bg-primary rounded-full overflow-hidden">
              <motion.div
                className="h-full bg-accent rounded-full w-1/3"
                animate={{ x: ["-100%", "400%"] }}
                transition={{
                  repeat: Infinity,
                  duration: 1.2,
                  ease: "easeInOut",
                }}
              />
            </div>
          </div>
        ) : status === "adding" ? (
          <div className="flex items-center gap-2">
            <Loader2 className="w-4 h-4 text-accent animate-spin" />
            <p className="text-sm text-text-muted">Registering project...</p>
          </div>
        ) : (
          <>
            <button
              onClick={handleOpenFolder}
              className="w-full flex items-center justify-center gap-2 px-4 py-2.5 text-sm font-medium bg-accent text-white rounded-md hover:bg-accent-hover transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              Choose folder
            </button>
            <p className="text-[11px] text-text-muted text-center">
              You can add more projects later from the Projects page.
            </p>
          </>
        )}

        {error && (
          <div className="flex items-start gap-2 text-xs text-danger">
            <AlertCircle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
            <span>{error}</span>
          </div>
        )}
      </div>

      <div className="flex gap-3">
        <button
          onClick={onNext}
          className={`flex-1 flex items-center justify-center gap-2 px-4 py-3 text-sm font-medium rounded-lg transition-colors ${
            status === "done"
              ? "bg-accent text-white hover:bg-accent-hover"
              : "bg-bg-card text-text-secondary border border-border hover:border-border-hover"
          }`}
        >
          {status === "done" ? "Continue" : "Skip for now"}
          <ChevronRight className="w-4 h-4" />
        </button>
      </div>
    </StepShell>
  );
}

// Step 3: Integrations (MCP + hooks + skill)
function StepIntegrations({ onNext }: { onNext: () => void }) {
  const [clients, setClients] = useState<AiClient[]>([]);
  const [hookInstalled, setHookInstalled] = useState(false);
  const [skillInstalled, setSkillInstalled] = useState(false);
  const [acting, setActing] = useState<string | null>(null);

  useEffect(() => {
    api.detectClients().then(setClients);
    api.getHookStatus().then(setHookInstalled);
    api.getSkillStatus().then(setSkillInstalled);
  }, []);

  const clientIcons: Record<string, React.ReactNode> = {
    "claude-code": <Terminal className="w-4 h-4" />,
    cursor: <MousePointer className="w-4 h-4" />,
    openclaw: <Cog className="w-4 h-4" />,
  };

  const installAll = async () => {
    setActing("all");
    try {
      for (const client of clients) {
        if (client.detected && !client.mcp_configured) {
          await api.installMcp(client.id);
        }
      }
      if (!hookInstalled) await api.installClaudeHook();
      if (!skillInstalled) await api.installSkill();

      const updated = await api.detectClients();
      setClients(updated);
      setHookInstalled(true);
      setSkillInstalled(true);
    } catch (e) {
      console.error(e);
    }
    setActing(null);
  };

  const toggle = async (
    id: string,
    install: () => Promise<void>,
    onDone: () => void
  ) => {
    setActing(id);
    try {
      await install();
      onDone();
    } catch (e) {
      console.error(e);
    }
    setActing(null);
  };

  const allDone =
    clients.filter((c) => c.detected).every((c) => c.mcp_configured) &&
    hookInstalled &&
    skillInstalled;

  const items = [
    ...clients
      .filter((c) => c.detected)
      .map((c) => ({
        id: c.id,
        icon: clientIcons[c.id] || <Cog className="w-4 h-4" />,
        name: `${c.name} MCP`,
        desc: "Graph, deps, impact, memory tools",
        done: c.mcp_configured,
        onInstall: async () => {
          await api.installMcp(c.id);
          const updated = await api.detectClients();
          setClients(updated);
        },
      })),
    {
      id: "hook",
      icon: <Zap className="w-4 h-4" />,
      name: "Search Hook",
      desc: "Redirects grep/find to graph search",
      done: hookInstalled,
      onInstall: async () => {
        await api.installClaudeHook();
        setHookInstalled(true);
      },
    },
    {
      id: "skill",
      icon: <BookOpen className="w-4 h-4" />,
      name: "Claude Code Skill",
      desc: "Teaches Claude the graph-first rule",
      done: skillInstalled,
      onInstall: async () => {
        await api.installSkill();
        setSkillInstalled(true);
      },
    },
  ];

  return (
    <StepShell step={3}>
      <div className="flex flex-col items-center gap-3">
        <div className="w-12 h-12 rounded-xl bg-accent/10 flex items-center justify-center">
          <Zap className="w-6 h-6 text-accent" />
        </div>
        <h2 className="text-xl font-semibold text-text-primary">
          Connect your tools
        </h2>
        <p className="text-sm text-text-muted text-center max-w-sm">
          Install MCP server, search hooks, and skills for your AI coding tools.
        </p>
      </div>

      <div className="space-y-2">
        {items.map((item) => (
          <div
            key={item.id}
            className="flex items-center justify-between bg-bg-card border border-border rounded-lg px-4 py-3"
          >
            <div className="flex items-center gap-3">
              <div className="text-text-secondary">{item.icon}</div>
              <div>
                <p className="text-sm font-medium text-text-primary">
                  {item.name}
                </p>
                <p className="text-xs text-text-muted">{item.desc}</p>
              </div>
            </div>
            {acting === item.id || acting === "all" ? (
              <Loader2 className="w-4 h-4 text-accent animate-spin" />
            ) : item.done ? (
              <Check className="w-4 h-4 text-success" />
            ) : (
              <button
                onClick={() => toggle(item.id, item.onInstall, () => {})}
                className="text-xs font-medium text-accent hover:text-accent-hover transition-colors"
              >
                Install
              </button>
            )}
          </div>
        ))}
      </div>

      {!allDone && (
        <button
          onClick={installAll}
          disabled={acting !== null}
          className="w-full flex items-center justify-center gap-2 px-4 py-2.5 text-sm font-medium bg-accent/10 text-accent rounded-lg hover:bg-accent/20 disabled:opacity-50 transition-colors"
        >
          {acting === "all" ? (
            <>
              <Loader2 className="w-4 h-4 animate-spin" />
              Installing...
            </>
          ) : (
            "Install all"
          )}
        </button>
      )}

      <button
        onClick={onNext}
        className={`w-full flex items-center justify-center gap-2 px-4 py-3 text-sm font-medium rounded-lg transition-colors ${
          allDone
            ? "bg-accent text-white hover:bg-accent-hover"
            : "bg-bg-card text-text-secondary border border-border hover:border-border-hover"
        }`}
      >
        {allDone ? "Continue" : "Skip for now"}
        <ChevronRight className="w-4 h-4" />
      </button>
    </StepShell>
  );
}

// Step 4: Embeddings
function StepEmbeddings({ onNext }: { onNext: () => void }) {
  const [mode, setMode] = useState("local");
  const [voyageKey, setVoyageKey] = useState("");
  const [openaiKey, setOpenaiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    api.getEmbeddingSettings().then((s) => {
      if (s.mode !== "disabled") setMode(s.mode);
      setVoyageKey(s.voyage_key || "");
      setOpenaiKey(s.openai_key || "");
    });
  }, []);

  const handleSave = async () => {
    setSaving(true);
    const settings: EmbeddingSettingsInput = {
      mode,
      model: null,
      openai_base_url: null,
      openai_key: mode === "openai" ? openaiKey || null : null,
      voyage_key: mode === "voyage" ? voyageKey || null : null,
    };
    await api.setEmbeddingSettings(settings);
    setSaving(false);
    setSaved(true);
  };

  const providers = [
    {
      id: "local",
      name: "Local",
      desc: "all-MiniLM-L6-v2 — no API key, runs on device",
      badge: "Free",
    },
    {
      id: "voyage",
      name: "Voyage AI",
      desc: "voyage-code-3 — code-specialized, best quality",
      badge: "Recommended",
    },
    {
      id: "openai",
      name: "OpenAI",
      desc: "text-embedding-3-small — general purpose",
      badge: null,
    },
    {
      id: "disabled",
      name: "Disabled",
      desc: "Text search only, no semantic embeddings",
      badge: null,
    },
  ];

  return (
    <StepShell step={4}>
      <div className="flex flex-col items-center gap-3">
        <div className="w-12 h-12 rounded-xl bg-accent/10 flex items-center justify-center">
          <Brain className="w-6 h-6 text-accent" />
        </div>
        <h2 className="text-xl font-semibold text-text-primary">
          Semantic search
        </h2>
        <p className="text-sm text-text-muted text-center max-w-sm">
          Embeddings let you search by meaning — "money transfer" finds{" "}
          <code className="text-text-secondary">payment_service</code>. Pick a
          provider.
        </p>
      </div>

      <div className="space-y-2">
        {providers.map((p) => (
          <button
            key={p.id}
            onClick={() => {
              setMode(p.id);
              setSaved(false);
            }}
            className={`w-full flex items-center justify-between px-4 py-3 rounded-lg border transition-colors text-left ${
              mode === p.id
                ? "border-accent bg-accent/5"
                : "border-border bg-bg-card hover:border-border-hover"
            }`}
          >
            <div>
              <div className="flex items-center gap-2">
                <p className="text-sm font-medium text-text-primary">
                  {p.name}
                </p>
                {p.badge && (
                  <span className="text-[10px] font-medium px-1.5 py-0.5 rounded-full bg-accent/10 text-accent">
                    {p.badge}
                  </span>
                )}
              </div>
              <p className="text-xs text-text-muted mt-0.5">{p.desc}</p>
            </div>
            <div
              className={`w-4 h-4 rounded-full border-2 transition-colors ${
                mode === p.id
                  ? "border-accent bg-accent"
                  : "border-border"
              }`}
            >
              {mode === p.id && (
                <Check className="w-3 h-3 text-white" />
              )}
            </div>
          </button>
        ))}
      </div>

      {mode === "voyage" && (
        <div className="space-y-1.5">
          <label className="block text-xs font-medium text-text-secondary">
            Voyage API Key
          </label>
          <input
            type="password"
            value={voyageKey}
            onChange={(e) => {
              setVoyageKey(e.target.value);
              setSaved(false);
            }}
            placeholder="pa-..."
            className="w-full text-xs bg-bg-card px-3 py-2 rounded border border-border text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent"
          />
        </div>
      )}

      {mode === "openai" && (
        <div className="space-y-1.5">
          <label className="block text-xs font-medium text-text-secondary">
            OpenAI API Key
          </label>
          <input
            type="password"
            value={openaiKey}
            onChange={(e) => {
              setOpenaiKey(e.target.value);
              setSaved(false);
            }}
            placeholder="sk-..."
            className="w-full text-xs bg-bg-card px-3 py-2 rounded border border-border text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent"
          />
        </div>
      )}

      {!saved ? (
        <button
          onClick={handleSave}
          disabled={saving}
          className="w-full flex items-center justify-center gap-2 px-4 py-3 text-sm font-medium bg-accent text-white rounded-lg hover:bg-accent-hover disabled:opacity-50 transition-colors"
        >
          {saving ? (
            <>
              <Loader2 className="w-4 h-4 animate-spin" />
              Saving...
            </>
          ) : (
            "Save & continue"
          )}
        </button>
      ) : (
        <button
          onClick={onNext}
          className="w-full flex items-center justify-center gap-2 px-4 py-3 text-sm font-medium bg-accent text-white rounded-lg hover:bg-accent-hover transition-colors"
        >
          Continue
          <ChevronRight className="w-4 h-4" />
        </button>
      )}
    </StepShell>
  );
}

// Step 5: Done
function StepDone({ onComplete }: { onComplete: () => void }) {
  const [summary, setSummary] = useState<{
    cli: boolean;
    projects: number;
    mcp: number;
    hook: boolean;
    skill: boolean;
    embeddings: string;
  } | null>(null);

  useEffect(() => {
    Promise.all([
      api.checkCliInstalled(),
      api.listProjects(),
      api.detectClients(),
      api.getHookStatus(),
      api.getSkillStatus(),
      api.getEmbeddingSettings(),
    ]).then(([cli, projects, clients, hook, skill, emb]) => {
      setSummary({
        cli: cli.installed,
        projects: projects.length,
        mcp: clients.filter((c) => c.mcp_configured).length,
        hook,
        skill,
        embeddings: emb.mode,
      });
    });
  }, []);

  return (
    <StepShell step={TOTAL_STEPS - 1}>
      <div className="flex flex-col items-center gap-5">
        <div className="w-16 h-16 rounded-2xl bg-success/10 flex items-center justify-center">
          <Sparkles className="w-8 h-8 text-success" />
        </div>
        <h2 className="text-2xl font-semibold text-text-primary">
          You're all set
        </h2>
        <p className="text-sm text-text-muted text-center max-w-sm">
          GraphMind is ready. Your AI tools now have structural understanding of
          your code.
        </p>
      </div>

      {summary && (
        <div className="bg-bg-card border border-border rounded-lg divide-y divide-border">
          {[
            {
              label: "CLI Engine",
              value: summary.cli ? "Installed" : "Not installed",
              ok: summary.cli,
            },
            {
              label: "Projects",
              value:
                summary.projects > 0
                  ? `${summary.projects} indexed`
                  : "None yet",
              ok: summary.projects > 0,
            },
            {
              label: "MCP Servers",
              value:
                summary.mcp > 0
                  ? `${summary.mcp} configured`
                  : "None",
              ok: summary.mcp > 0,
            },
            {
              label: "Search Hook",
              value: summary.hook ? "Active" : "Not installed",
              ok: summary.hook,
            },
            {
              label: "Skill",
              value: summary.skill ? "Active" : "Not installed",
              ok: summary.skill,
            },
            {
              label: "Embeddings",
              value:
                summary.embeddings === "disabled"
                  ? "Disabled"
                  : summary.embeddings,
              ok: summary.embeddings !== "disabled",
            },
          ].map((row) => (
            <div
              key={row.label}
              className="flex items-center justify-between px-4 py-2.5"
            >
              <span className="text-sm text-text-secondary">{row.label}</span>
              <span
                className={`text-sm font-medium ${
                  row.ok ? "text-success" : "text-text-muted"
                }`}
              >
                {row.value}
              </span>
            </div>
          ))}
        </div>
      )}

      <button
        onClick={onComplete}
        className="w-full flex items-center justify-center gap-2 px-4 py-3 text-sm font-medium bg-accent text-white rounded-lg hover:bg-accent-hover transition-colors"
      >
        Open GraphMind
        <ChevronRight className="w-4 h-4" />
      </button>
    </StepShell>
  );
}

export function Setup({ onComplete }: SetupProps) {
  const [step, setStep] = useState(0);

  switch (step) {
    case 0:
      return <StepWelcome onNext={() => setStep(1)} />;
    case 1:
      return (
        <StepInstallCli
          onNext={() => setStep(2)}
          onSkip={() => setStep(2)}
        />
      );
    case 2:
      return <StepAddProject onNext={() => setStep(3)} />;
    case 3:
      return <StepIntegrations onNext={() => setStep(4)} />;
    case 4:
      return <StepEmbeddings onNext={() => setStep(5)} />;
    case 5:
      return <StepDone onComplete={onComplete} />;
    default:
      return null;
  }
}
