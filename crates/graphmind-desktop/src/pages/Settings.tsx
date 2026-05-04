import { useState, useEffect } from "react";
import { api, AppUpdateInfo, ProjectInfo } from "../lib/tauri";
import { Plus, X, Save, Brain, Download, RefreshCw } from "lucide-react";

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
  const [appVersion, setAppVersion] = useState<string | null>(null);
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
    api.getAppVersion().then(setAppVersion).catch(() => {});
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
              <option value="local">Local (all-MiniLM-L6-v2, no API key)</option>
              <option value="openai">OpenAI</option>
              <option value="voyage">Voyage AI (code-specialized)</option>
            </select>
          </div>

          <div>
            <label className="block text-xs font-medium text-text-secondary mb-1">Model override (optional)</label>
            <input
              type="text"
              value={embModel}
              onChange={(e) => setEmbModel(e.target.value)}
              placeholder={embMode === "openai" ? "text-embedding-3-small" : embMode === "voyage" ? "voyage-code-3" : "all-MiniLM-L6-v2"}
              className="w-full text-xs bg-bg-card px-3 py-1.5 rounded border border-border text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent"
            />
          </div>

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

      {/* Updates */}
      <h2 className="text-lg font-semibold text-text-primary flex items-center gap-2">
        <Download className="w-4 h-4" />
        Updates
      </h2>

      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-xs text-text-secondary">
              Current version:{" "}
              <span className="text-text-primary font-medium">
                {appVersion ?? updateInfo?.current_version ?? "—"}
              </span>
            </p>
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
          </div>
        </div>
        <p className="text-xs text-text-muted">
          Updates the desktop app. Homebrew CLI installations are updated separately via <code>brew upgrade graphmind</code>.
        </p>
      </section>
    </div>
  );
}
