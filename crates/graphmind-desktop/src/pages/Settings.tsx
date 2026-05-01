import { useState, useEffect } from "react";
import { api, ProjectInfo } from "../lib/tauri";
import { Plus, X, Save, Brain } from "lucide-react";

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

  const saveEmbeddings = async () => {
    setEmbSaving(true);
    await api.setEmbeddingSettings({
      mode: embMode,
      model: embModel || null,
      openai_base_url: embBaseUrl || null,
      openai_key: embOpenaiKey || null,
      voyage_key: embVoyageKey || null,
    });
    setEmbSaving(false);
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
      </section>
    </div>
  );
}
