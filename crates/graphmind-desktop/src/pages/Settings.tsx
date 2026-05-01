import { useState, useEffect } from "react";
import { api, ProjectInfo } from "../lib/tauri";
import { Plus, X, Save, Zap, GitBranch, BookOpen, Check } from "lucide-react";

export function Settings() {
  const [projects, setProjects] = useState<ProjectInfo[]>([]);
  const [globalExcludes, setGlobalExcludes] = useState<string[]>([]);
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [projectExcludes, setProjectExcludes] = useState<string[]>([]);
  const [newGlobal, setNewGlobal] = useState("");
  const [newProject, setNewProject] = useState("");
  const [saving, setSaving] = useState(false);
  const [hookEnabled, setHookEnabled] = useState(false);
  const [hookLoading, setHookLoading] = useState(false);
  const [gitHookEnabled, setGitHookEnabled] = useState(false);
  const [gitHookLoading, setGitHookLoading] = useState(false);
  const [skillInstalled, setSkillInstalled] = useState(false);
  const [skillLoading, setSkillLoading] = useState(false);

  useEffect(() => {
    api.listProjects().then(setProjects);
    api.getExcludes().then((s) => setGlobalExcludes(s.global));
    api.getHookStatus().then(setHookEnabled);
    api.getGitHookStatus().then(setGitHookEnabled);
    api.getSkillStatus().then(setSkillInstalled);
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
        await api.uninstallGitHook(selectedProject || undefined);
        setGitHookEnabled(false);
      } else {
        await api.installGitHook(selectedProject || undefined);
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

  return (
    <div className="p-6 max-w-3xl mx-auto space-y-8">
      <h1 className="text-lg font-semibold text-text-primary">Settings</h1>

      {/* Claude Code Integration */}
      <section className="space-y-3">
        <h2 className="text-sm font-medium text-text-primary">Claude Code Integration</h2>

        {/* Search Hook */}
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

        {/* Git Hooks */}
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

        {/* Skill */}
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
    </div>
  );
}
