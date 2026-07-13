import { useState, useEffect } from "react";
import { api, AppUpdateInfo, ProjectInfo, RemoteSettings } from "../lib/tauri";
import { Plus, X, Save, Brain, Download, RefreshCw, Power, Zap } from "lucide-react";

export function Settings() {
  const [projects, setProjects] = useState<ProjectInfo[]>([]);
  const [globalExcludes, setGlobalExcludes] = useState<string[]>([]);
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [projectExcludes, setProjectExcludes] = useState<string[]>([]);
  const [newGlobal, setNewGlobal] = useState("");
  const [newProject, setNewProject] = useState("");
  const [saving, setSaving] = useState(false);
  const [embMode, setEmbMode] = useState("disabled");
  const [embModel, setEmbModel] = useState("");
  const [embBaseUrl, setEmbBaseUrl] = useState("");
  const [embOpenaiKey, setEmbOpenaiKey] = useState("");
  const [embVoyageKey, setEmbVoyageKey] = useState("");
  const [embSaving, setEmbSaving] = useState(false);
  const [remoteSettings, setRemoteSettings] = useState<RemoteSettings | null>(null);
  const [remoteSaving, setRemoteSaving] = useState(false);
  const [remoteError, setRemoteError] = useState<string | null>(null);
  const [licenseKey, setLicenseKey] = useState("");
  const [licenseStatus, setLicenseStatus] = useState<{ display: string; tier: string; is_expired: boolean } | null>(null);
  const [licenseActivating, setLicenseActivating] = useState(false);
  const [licenseError, setLicenseError] = useState<string | null>(null);
  const [launchAtLogin, setLaunchAtLogin] = useState(false);
  const [buildAllOnStartup, setBuildAllOnStartup] = useState(false);
  const [appVersion, setAppVersion] = useState<string | null>(null);
  const [cliVersion, setCliVersion] = useState<string | null>(null);
  const [cliUpdateAvailable, setCliUpdateAvailable] = useState(false);
  const [cliUpdating, setCliUpdating] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<AppUpdateInfo | null>(null);
  const [updateChecking, setUpdateChecking] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [updating, setUpdating] = useState(false);
  const [updateDone, setUpdateDone] = useState<string | null>(null);

  useEffect(() => {
    api.listProjects().then(setProjects);
    api.getExcludes().then((s) => setGlobalExcludes(s.global));
    api.getEmbeddingSettings().then((s) => {
      setEmbMode(s.mode);
      setEmbModel(s.model || "");
      setEmbBaseUrl(s.openai_base_url || "");
      setEmbOpenaiKey(s.openai_key || "");
      setEmbVoyageKey(s.voyage_key || "");
    });
    api.getStartupSettings().then((s) => {
      setLaunchAtLogin(s.launch_at_login);
      setBuildAllOnStartup(s.build_all_on_startup);
    }).catch(() => {});
    api.getRemoteSettings().then(setRemoteSettings).catch(() => {});
    api.getLicenseStatus().then(setLicenseStatus).catch(() => {});
    api.getAppVersion().then(setAppVersion).catch(() => {});
    api.checkCliInstalled().then((s) => setCliVersion(s.version ?? null)).catch(() => {});
    api.checkCliUpdate().then((u) => { if (u.update_available) setCliUpdateAvailable(true); }).catch(() => {});
    setUpdateChecking(true);
    api.checkAppUpdate()
      .then((info) => { setUpdateInfo(info); setUpdateError(null); })
      .catch((e) => setUpdateError(String(e)))
      .finally(() => setUpdateChecking(false));
  }, []);

  useEffect(() => {
    if (selectedProject) {
      api.getExcludes(selectedProject).then((s) => setProjectExcludes(s.project));
    } else {
      setProjectExcludes([]);
    }
  }, [selectedProject]);

  const saveGlobal = async () => {
    setSaving(true);
    await api.setGlobalExcludes(globalExcludes);
    setSaving(false);
  };

  const saveProject = async () => {
    if (!selectedProject) return;
    setSaving(true);
    await api.setProjectExcludes(selectedProject, projectExcludes);
    setSaving(false);
  };

  const addGlobal = () => {
    const val = newGlobal.trim();
    if (val && !globalExcludes.includes(val)) {
      setGlobalExcludes([...globalExcludes, val]);
      setNewGlobal("");
    }
  };

  const addProject = () => {
    const val = newProject.trim();
    if (val && !projectExcludes.includes(val)) {
      setProjectExcludes([...projectExcludes, val]);
      setNewProject("");
    }
  };

  const [embPrompt, setEmbPrompt] = useState<string[] | null>(null);
  const [embRunning, setEmbRunning] = useState(false);

  const saveEmbeddings = async () => {
    setEmbSaving(true);
    const result = await api.setEmbeddingSettings({
      mode: embMode,
      model: embModel || null,
      openai_base_url: embBaseUrl || null,
      openai_key: embOpenaiKey || null,
      voyage_key: embVoyageKey || null,
    });
    setEmbSaving(false);
    if (result.projects_needing_embedding.length > 0) {
      setEmbPrompt(result.projects_needing_embedding);
    }
  };

  const runEmbeddings = async () => {
    if (!embPrompt) return;
    setEmbRunning(true);
    try {
      await api.embedProjects(embPrompt);
    } finally {
      setEmbRunning(false);
      setEmbPrompt(null);
    }
  };

  return (
    <div className="p-6 max-w-3xl mx-auto space-y-8">
      <h1 className="text-lg font-semibold text-text-primary">Settings</h1>

      <h2 className="text-lg font-semibold text-text-primary">Exclusions</h2>

      {/* Global Excludes */}
      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-medium text-text-primary">Global Excludes</h2>
          <button
            onClick={saveGlobal}
            disabled={saving}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-accent text-white rounded-md hover:bg-accent/90 disabled:opacity-50"
          >
            <Save className="w-3 h-3" />
            Save
          </button>
        </div>
        <p className="text-xs text-text-muted">
          Patterns applied to all projects. Use directory names, relative paths, or file patterns (*.min.js).
        </p>
        <div className="space-y-1.5">
          {globalExcludes.map((ex, i) => (
            <div key={i} className="flex items-center gap-2 group">
              <code className="flex-1 text-xs bg-bg-card px-3 py-1.5 rounded border border-border text-text-secondary">
                {ex}
              </code>
              <button
                onClick={() => setGlobalExcludes(globalExcludes.filter((_, idx) => idx !== i))}
                className="opacity-0 group-hover:opacity-100 text-text-muted hover:text-red-400 transition-opacity"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            </div>
          ))}
        </div>
        <div className="flex gap-2">
          <input
            type="text"
            value={newGlobal}
            onChange={(e) => setNewGlobal(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addGlobal()}
            placeholder="e.g. vendor, *.min.js, logs/"
            className="flex-1 text-xs bg-bg-card px-3 py-1.5 rounded border border-border text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent"
          />
          <button
            onClick={addGlobal}
            className="flex items-center gap-1 px-3 py-1.5 text-xs font-medium border border-border rounded-md text-text-secondary hover:text-text-primary hover:border-accent"
          >
            <Plus className="w-3 h-3" />
            Add
          </button>
        </div>
      </section>

      {/* Project Excludes */}
      <section className="space-y-3">
        <h2 className="text-sm font-medium text-text-primary">Project Excludes</h2>
        <p className="text-xs text-text-muted">
          Paths relative to the project root. Only applied during indexing of the selected project.
        </p>
        <select
          value={selectedProject || ""}
          onChange={(e) => setSelectedProject(e.target.value || null)}
          className="w-full text-xs bg-bg-card px-3 py-2 rounded border border-border text-text-primary focus:outline-none focus:border-accent"
        >
          <option value="">Select a project...</option>
          {projects.map((p) => (
            <option key={p.slug} value={p.slug}>
              {p.slug}
            </option>
          ))}
        </select>

        {selectedProject && (
          <>
            <div className="flex items-center justify-between">
              <span className="text-xs text-text-muted">{projectExcludes.length} exclusion(s)</span>
              <button
                onClick={saveProject}
                disabled={saving}
                className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-accent text-white rounded-md hover:bg-accent/90 disabled:opacity-50"
              >
                <Save className="w-3 h-3" />
                Save
              </button>
            </div>
            <div className="space-y-1.5">
              {projectExcludes.map((ex, i) => (
                <div key={i} className="flex items-center gap-2 group">
                  <code className="flex-1 text-xs bg-bg-card px-3 py-1.5 rounded border border-border text-text-secondary">
                    {ex}
                  </code>
                  <button
                    onClick={() => setProjectExcludes(projectExcludes.filter((_, idx) => idx !== i))}
                    className="opacity-0 group-hover:opacity-100 text-text-muted hover:text-red-400 transition-opacity"
                  >
                    <X className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
            <div className="flex gap-2">
              <input
                type="text"
                value={newProject}
                onChange={(e) => setNewProject(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && addProject()}
                placeholder="e.g. app/deployment_packages, test/fixtures"
                className="flex-1 text-xs bg-bg-card px-3 py-1.5 rounded border border-border text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent"
              />
              <button
                onClick={addProject}
                className="flex items-center gap-1 px-3 py-1.5 text-xs font-medium border border-border rounded-md text-text-secondary hover:text-text-primary hover:border-accent"
              >
                <Plus className="w-3 h-3" />
                Add
              </button>
            </div>
          </>
        )}
      </section>

      {/* Embeddings */}
      <h2 className="text-lg font-semibold text-text-primary flex items-center gap-2">
        <Brain className="w-4 h-4" />
        Embeddings
      </h2>

      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <p className="text-xs text-text-muted">
            Semantic vector search over symbols. Computed automatically during build.
          </p>
          <button
            onClick={saveEmbeddings}
            disabled={embSaving}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-accent text-white rounded-md hover:bg-accent/90 disabled:opacity-50"
          >
            <Save className="w-3 h-3" />
            Save
          </button>
        </div>

        <div className="space-y-3">
          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">Provider</label>
            <select
              value={embMode}
              onChange={(e) => setEmbMode(e.target.value)}
              className="w-full text-xs bg-bg-card px-3 py-2 rounded border border-border text-text-primary focus:outline-none focus:border-accent"
            >
              <option value="disabled">Disabled</option>
              <option value="local">Local (nomic-embed-text-v1.5, no API key)</option>
              <option value="openai">OpenAI</option>
              <option value="voyage">Voyage AI (code-specialized)</option>
            </select>
            {embMode !== "disabled" && (
              <p className="text-xs text-text-muted mt-1">
                Using:{" "}
                <code className="text-text-secondary">
                  {embModel ||
                    (embMode === "openai"
                      ? "text-embedding-3-small"
                      : embMode === "voyage"
                      ? "voyage-code-3"
                      : "nomic-embed-text-v1.5")}
                </code>
              </p>
            )}
          </div>

          {embMode === "local" && (
            <div className="p-3 rounded-md border border-border bg-bg-card text-xs text-text-muted space-y-1">
              <p>The ONNX model (<code className="text-text-secondary">nomic-embed-text-v1.5</code>) is downloaded automatically on first <code className="text-text-secondary">graphmind build</code>. No API key needed.</p>
              <p>Model is cached at <code className="text-text-secondary">~/.graphmind/models/</code>.</p>
              <p className="text-warning">⚠ On large projects, the first embedding build can take several minutes. Subsequent builds only process new files.</p>
            </div>
          )}

          {embMode !== "disabled" && (
            <div>
              <label className="block text-xs font-medium text-text-secondary mb-1">Model override (optional)</label>
              <input
                type="text"
                value={embModel}
                onChange={(e) => setEmbModel(e.target.value)}
                placeholder={embMode === "openai" ? "text-embedding-3-small" : embMode === "voyage" ? "voyage-code-3" : "nomic-embed-text-v1.5"}
                className="w-full text-xs bg-bg-card px-3 py-1.5 rounded border border-border text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent"
              />
            </div>
          )}

          {embMode === "openai" && (
            <>
              <div>
                <label className="block text-xs font-medium text-text-secondary mb-1">OpenAI API Key</label>
                <input
                  type="password"
                  value={embOpenaiKey}
                  onChange={(e) => setEmbOpenaiKey(e.target.value)}
                  placeholder="sk-..."
                  className="w-full text-xs bg-bg-card px-3 py-1.5 rounded border border-border text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent"
                />
              </div>
              <div>
                <label className="block text-xs font-medium text-text-secondary mb-1">Base URL (optional, for Azure/proxies)</label>
                <input
                  type="text"
                  value={embBaseUrl}
                  onChange={(e) => setEmbBaseUrl(e.target.value)}
                  placeholder="https://api.openai.com/v1"
                  className="w-full text-xs bg-bg-card px-3 py-1.5 rounded border border-border text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent"
                />
              </div>
            </>
          )}

          {embMode === "voyage" && (
            <div>
              <label className="block text-xs font-medium text-text-secondary mb-1">Voyage API Key</label>
              <input
                type="password"
                value={embVoyageKey}
                onChange={(e) => setEmbVoyageKey(e.target.value)}
                placeholder="pa-..."
                className="w-full text-xs bg-bg-card px-3 py-1.5 rounded border border-border text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent"
              />
            </div>
          )}
        </div>

        {embPrompt && (
          <div className="p-4 rounded-lg border border-accent/40 bg-accent/5 space-y-3">
            <p className="text-sm text-text-primary">
              {embPrompt.length} project{embPrompt.length > 1 ? "s" : ""} already built but missing embeddings:
            </p>
            <ul className="text-xs text-text-secondary space-y-0.5 pl-4 list-disc">
              {embPrompt.map((slug) => (
                <li key={slug}>{slug}</li>
              ))}
            </ul>
            <div className="flex gap-2">
              <button
                onClick={runEmbeddings}
                disabled={embRunning}
                className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-accent text-white rounded-md hover:bg-accent/90 disabled:opacity-50"
              >
                <Brain className="w-3 h-3" />
                {embRunning ? "Generating..." : "Generate embeddings now"}
              </button>
              <button
                onClick={() => setEmbPrompt(null)}
                disabled={embRunning}
                className="px-3 py-1.5 text-xs font-medium border border-border rounded-md text-text-secondary hover:text-text-primary disabled:opacity-50"
              >
                Later
              </button>
            </div>
          </div>
        )}
      </section>

      {/* Remote & License */}
      <h2 className="text-lg font-semibold text-text-primary flex items-center gap-2">
        <Zap className="w-4 h-4" />
        Remote & License
      </h2>

      <section className="space-y-4">
        {/* License key + status */}
        <div className="space-y-2">
          <p className="text-xs font-medium text-text-secondary">License</p>
          {licenseStatus && (
            <div className="flex items-center gap-2">
              <span className={`text-xs px-2 py-0.5 rounded-full font-medium ${
                licenseStatus.tier === "free" ? "bg-border text-text-muted" :
                licenseStatus.tier === "embeddings" ? "bg-blue-500/15 text-blue-400" :
                licenseStatus.tier === "pro" ? "bg-accent/15 text-accent" :
                "bg-purple-500/15 text-purple-400"
              }`}>
                {licenseStatus.tier.charAt(0).toUpperCase() + licenseStatus.tier.slice(1)}
              </span>
              <span className="text-xs text-text-muted">{licenseStatus.display}</span>
              {licenseStatus.is_expired && (
                <span className="text-xs text-red-400 font-medium">— expired</span>
              )}
            </div>
          )}
          <div className="flex gap-2">
            <input
              type="password"
              value={licenseKey}
              onChange={(e) => setLicenseKey(e.target.value)}
              placeholder="gm_live_..."
              className="flex-1 text-xs bg-bg-card px-3 py-1.5 rounded border border-border text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent"
            />
            <button
              onClick={async () => {
                if (!licenseKey.trim()) return;
                setLicenseActivating(true);
                setLicenseError(null);
                try {
                  const status = await api.activateLicense(licenseKey.trim());
                  setLicenseStatus(status);
                  setLicenseKey("");
                  api.getRemoteSettings().then(setRemoteSettings).catch(() => {});
                } catch (e) {
                  setLicenseError(String(e));
                } finally {
                  setLicenseActivating(false);
                }
              }}
              disabled={licenseActivating || !licenseKey.trim()}
              className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-accent text-white rounded-md hover:bg-accent/90 disabled:opacity-50"
            >
              {licenseActivating ? "Activating..." : "Activate"}
            </button>
          </div>
          {licenseError && <p className="text-xs text-red-400">{licenseError}</p>}
        </div>

        {/* Remote mode — upsell for free, controls for paid */}
        {remoteSettings && (
          <div className="space-y-2 border-t border-border pt-4">
            <p className="text-xs font-medium text-text-secondary">Remote mode</p>

            {licenseStatus?.tier === "free" ? (
              <div className="p-4 rounded-lg border border-accent/30 bg-accent/5 space-y-3">
                <p className="text-sm font-medium text-text-primary">Unlock server-side intelligence</p>
                <ul className="text-xs text-text-secondary space-y-1.5">
                  <li className="flex items-start gap-2">
                    <span className="text-blue-400 mt-0.5">◆</span>
                    <span><span className="font-medium text-text-primary">Embeddings tier</span> — server-side semantic search, no GPU or API key needed</span>
                  </li>
                  <li className="flex items-start gap-2">
                    <span className="text-accent mt-0.5">◆</span>
                    <span><span className="font-medium text-text-primary">Pro / Team</span> — graph sync + remote MCP SSE, accessible from any machine</span>
                  </li>
                </ul>
                <button
                  onClick={() => api.openUpgradePage()}
                  className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-accent text-white rounded-md hover:bg-accent/90"
                >
                  <Zap className="w-3 h-3" />
                  View plans at getgraphmind.com
                </button>
              </div>
            ) : (
              <div className="space-y-2">
                {/* Embed toggle — available for embeddings+ */}
                <div className="flex items-center justify-between py-2 border-b border-border">
                  <div>
                    <p className="text-sm text-text-primary">Server-side embeddings</p>
                    <p className="text-xs text-text-muted mt-0.5">Semantic search via GraphMind server. No local GPU or API key needed.</p>
                  </div>
                  <button
                    role="switch"
                    aria-checked={remoteSettings.mode === "embed" || remoteSettings.mode === "full"}
                    disabled={remoteSaving}
                    onClick={async () => {
                      const isOn = remoteSettings.mode === "embed" || remoteSettings.mode === "full";
                      const next = isOn ? "off" : "embed";
                      setRemoteSaving(true);
                      setRemoteError(null);
                      try {
                        await api.setRemoteMode(next);
                        const s = await api.getRemoteSettings();
                        setRemoteSettings(s);
                      } catch (e) {
                        setRemoteError(String(e));
                      } finally {
                        setRemoteSaving(false);
                      }
                    }}
                    className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors focus:outline-none disabled:opacity-50 ${
                      remoteSettings.mode === "embed" || remoteSettings.mode === "full" ? "bg-accent" : "bg-border"
                    }`}
                  >
                    <span className={`inline-block h-3.5 w-3.5 transform rounded-full bg-white shadow transition-transform ${
                      remoteSettings.mode === "embed" || remoteSettings.mode === "full" ? "translate-x-4" : "translate-x-1"
                    }`} />
                  </button>
                </div>

                {/* Full mode toggle — Pro/Team only */}
                {(licenseStatus?.tier === "pro" || licenseStatus?.tier === "team") && (
                  <div className="flex items-center justify-between py-2">
                    <div>
                      <p className="text-sm text-text-primary">Full remote mode</p>
                      <p className="text-xs text-text-muted mt-0.5">Graph sync + remote MCP SSE. Enables cloud-based code intelligence.</p>
                      {remoteSettings.last_sync_at && (
                        <p className="text-xs text-text-muted mt-0.5">Last sync: {remoteSettings.last_sync_at}</p>
                      )}
                      {remoteSettings.mode === "full" && !remoteSettings.last_sync_at && (
                        <p className="text-xs text-yellow-400 mt-0.5">Never synced — run graphmind build to apply</p>
                      )}
                    </div>
                    <button
                      role="switch"
                      aria-checked={remoteSettings.mode === "full"}
                      disabled={remoteSaving}
                      onClick={async () => {
                        const next = remoteSettings.mode === "full" ? "embed" : "full";
                        setRemoteSaving(true);
                        setRemoteError(null);
                        try {
                          await api.setRemoteMode(next);
                          const s = await api.getRemoteSettings();
                          setRemoteSettings(s);
                        } catch (e) {
                          setRemoteError(String(e));
                        } finally {
                          setRemoteSaving(false);
                        }
                      }}
                      className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors focus:outline-none disabled:opacity-50 ${
                        remoteSettings.mode === "full" ? "bg-accent" : "bg-border"
                      }`}
                    >
                      <span className={`inline-block h-3.5 w-3.5 transform rounded-full bg-white shadow transition-transform ${
                        remoteSettings.mode === "full" ? "translate-x-4" : "translate-x-1"
                      }`} />
                    </button>
                  </div>
                )}

                {remoteError && <p className="text-xs text-red-400">{remoteError}</p>}

                {/* Upsell full mode for embeddings tier */}
                {licenseStatus?.tier === "embeddings" && (
                  <div className="p-3 rounded-md border border-border bg-bg-card text-xs text-text-muted space-y-1.5 mt-2">
                    <p className="font-medium text-text-secondary">Want full remote mode?</p>
                    <p>Graph sync + remote MCP SSE requires the Pro or Team plan.</p>
                    <button
                      onClick={() => api.openUpgradePage()}
                      className="text-accent hover:underline"
                    >
                      Upgrade at getgraphmind.com →
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </section>

      {/* Startup */}
      <h2 className="text-lg font-semibold text-text-primary flex items-center gap-2">
        <Power className="w-4 h-4" />
        Startup
      </h2>

      <section className="space-y-3">
        <div className="flex items-center justify-between py-2 border-b border-border">
          <div>
            <p className="text-sm text-text-primary">Launch at login</p>
            <p className="text-xs text-text-muted mt-0.5">Start GraphMind automatically when you log in to macOS.</p>
          </div>
          <button
            role="switch"
            aria-checked={launchAtLogin}
            onClick={async () => {
              const next = !launchAtLogin;
              try {
                await api.setLaunchAtLogin(next);
                setLaunchAtLogin(next);
              } catch (e) {
                console.error(e);
              }
            }}
            className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors focus:outline-none ${launchAtLogin ? "bg-accent" : "bg-border"}`}
          >
            <span className={`inline-block h-3.5 w-3.5 transform rounded-full bg-white shadow transition-transform ${launchAtLogin ? "translate-x-4" : "translate-x-1"}`} />
          </button>
        </div>

        <div className="flex items-center justify-between py-2">
          <div>
            <p className="text-sm text-text-primary">Update all projects on startup</p>
            <p className="text-xs text-text-muted mt-0.5">Automatically update all projects each time the app opens.</p>
          </div>
          <button
            role="switch"
            aria-checked={buildAllOnStartup}
            onClick={async () => {
              const next = !buildAllOnStartup;
              try {
                await api.setBuildAllOnStartup(next);
                setBuildAllOnStartup(next);
              } catch (e) {
                console.error(e);
              }
            }}
            className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors focus:outline-none ${buildAllOnStartup ? "bg-accent" : "bg-border"}`}
          >
            <span className={`inline-block h-3.5 w-3.5 transform rounded-full bg-white shadow transition-transform ${buildAllOnStartup ? "translate-x-4" : "translate-x-1"}`} />
          </button>
        </div>
      </section>

      {/* Updates */}
      <h2 className="text-lg font-semibold text-text-primary flex items-center gap-2">
        <Download className="w-4 h-4" />
        Updates
      </h2>

      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-xs text-text-secondary">
              App:{" "}
              <span className="text-text-primary font-medium">
                {appVersion ?? updateInfo?.current_version ?? "—"}
              </span>
            </p>
            {cliVersion && (
              <p className="text-xs text-text-secondary mt-0.5">
                CLI:{" "}
                <span className="text-text-primary font-medium">{cliVersion}</span>
                {cliUpdateAvailable && (
                  <span className="text-accent font-medium ml-1">— update available</span>
                )}
                {cliUpdating && (
                  <span className="text-text-muted ml-1">— updating...</span>
                )}
              </p>
            )}
            {updateDone && (
              <p className="text-xs text-green-500 mt-1">
                Updated to v{updateDone} — restart to apply
              </p>
            )}
            {!updateDone && updateInfo && updateInfo.update_available && updateInfo.new_version && (
              <p className="text-xs text-accent font-medium mt-1">
                v{updateInfo.new_version} available
              </p>
            )}
            {!updateDone && updateInfo && !updateInfo.update_available && (
              <p className="text-xs text-green-500 mt-1">Up to date</p>
            )}
            {updateError && (
              <p className="text-xs text-red-400 mt-1">Check failed: {updateError}</p>
            )}
          </div>
          <div className="flex gap-2">
            <button
              onClick={() => {
                setUpdateChecking(true);
                setUpdateError(null);
                api.checkAppUpdate()
                  .then((info) => { setUpdateInfo(info); })
                  .catch((e) => setUpdateError(String(e)))
                  .finally(() => setUpdateChecking(false));
              }}
              disabled={updateChecking}
              className="flex items-center gap-1 px-3 py-1.5 text-xs font-medium border border-border rounded-md text-text-secondary hover:text-text-primary disabled:opacity-50"
            >
              <RefreshCw className={`w-3 h-3 ${updateChecking ? "animate-spin" : ""}`} />
              {updateChecking ? "Checking..." : "Check"}
            </button>
            {updateInfo?.update_available && (
              <button
                onClick={async () => {
                  setUpdating(true);
                  setUpdateError(null);
                  try {
                    const v = await api.installAppUpdate();
                    setUpdateDone(v);
                    setUpdateInfo({ ...updateInfo, update_available: false });
                    // Also update CLI
                    setCliUpdating(true);
                    api.updateCli()
                      .then((s) => { setCliVersion(s.version ?? null); setCliUpdateAvailable(false); })
                      .catch(() => {})
                      .finally(() => setCliUpdating(false));
                  } catch (e) {
                    setUpdateError(String(e));
                  } finally {
                    setUpdating(false);
                  }
                }}
                disabled={updating}
                className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-accent text-white rounded-md hover:bg-accent/90 disabled:opacity-50"
              >
                <Download className="w-3 h-3" />
                {updating ? "Updating..." : "Update app"}
              </button>
            )}
            {cliUpdateAvailable && !updateInfo?.update_available && (
              <button
                onClick={async () => {
                  setCliUpdating(true);
                  try {
                    const s = await api.updateCli();
                    setCliVersion(s.version ?? null);
                    setCliUpdateAvailable(false);
                  } catch (e) {
                    setUpdateError(String(e));
                  } finally {
                    setCliUpdating(false);
                  }
                }}
                disabled={cliUpdating}
                className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-accent text-white rounded-md hover:bg-accent/90 disabled:opacity-50"
              >
                <Download className="w-3 h-3" />
                {cliUpdating ? "Updating..." : "Update CLI"}
              </button>
            )}
          </div>
        </div>
        {cliUpdating && (
          <p className="text-xs text-text-muted flex items-center gap-1">
            <RefreshCw className="w-3 h-3 animate-spin" /> Updating CLI...
          </p>
        )}
        <p className="text-xs text-text-muted">
          Updates the desktop app and CLI. Homebrew CLI installations are updated separately via <code>brew upgrade graphmind</code>.
        </p>
      </section>
    </div>
  );
}
