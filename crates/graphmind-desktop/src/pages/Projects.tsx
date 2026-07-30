import { useCallback, useState } from "react";
import { FolderPlus, RotateCw, Eye, EyeOff, Trash2, Zap, X } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { useProjects } from "../hooks/useProjects";
import { useTauriEvent } from "../hooks/useTauriEvent";
import { api, ProjectInfo } from "../lib/tauri";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { ProgressBar } from "../components/ui/ProgressBar";
import { Spinner } from "../components/ui/Spinner";

export function Projects() {
  const { projects, loading, refresh } = useProjects();
  const [building, setBuilding] = useState<string | null>(null);
  const [buildingPhase, setBuildingPhase] = useState<"indexing" | "embedding">("indexing");
  const [buildingAll, setBuildingAll] = useState(false);

  // Tracks which project is currently building so its card shows a progress bar.
  // Required for batch builds ("Rebuild All" / build-on-startup): those never go
  // through `handleBuild`, so without this listener no card would show progress
  // during the indexing phase.
  useTauriEvent<string>("indexing-started", useCallback((slug) => {
    setBuilding(slug);
    setBuildingPhase("indexing");
  }, []));

  useTauriEvent<string>("embedding-started", useCallback((slug) => {
    setBuilding(slug);
    setBuildingPhase("embedding");
  }, []));

  // `indexing-complete` / `indexing-cancelled` are emitted once per project, so
  // they must not clear `buildingAll` — during a batch the first project to
  // finish would stop the spinner while the others are still building.
  // `buildAll` owns that flag and clears it when the whole batch resolves.
  useTauriEvent<string>("indexing-complete", useCallback(() => {
    setBuilding(null);
    setBuildingPhase("indexing");
    refresh();
  }, [refresh]));

  useTauriEvent<string>("indexing-cancelled", useCallback(() => {
    setBuilding(null);
    setBuildingPhase("indexing");
    refresh();
  }, [refresh]));

  // Safety net: the backend signals the end of a batch explicitly, so the
  // spinner also clears when the batch was started somewhere else (e.g. the
  // build-on-startup path in App.tsx) rather than by this page.
  useTauriEvent<string[]>("build-all-complete", useCallback(() => {
    setBuildingAll(false);
    setBuilding(null);
    setBuildingPhase("indexing");
    refresh();
  }, [refresh]));

  const handleAdd = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      await api.addProject(selected as string);
      refresh();
    }
  };

  const handleBuild = async (slug: string) => {
    setBuilding(slug);
    try {
      await api.buildProject(slug, false);
    } catch {
      setBuilding(null);
    }
  };

  const handleCancel = async (slug: string) => {
    try {
      await api.cancelBuild(slug);
    } catch {
      // best-effort
    }
  };

  const buildAll = async (full: boolean) => {
    setBuildingAll(true);
    try {
      await api.buildAllProjects(full);
    } catch (e) {
      console.error(e);
    } finally {
      // The command resolves once every project is done, so this is the
      // authoritative end of the batch. Relying only on `indexing-complete`
      // leaves the spinner stuck whenever the last project is cancelled or
      // the command errors out before emitting it.
      setBuildingAll(false);
      setBuilding(null);
      setBuildingPhase("indexing");
      refresh();
    }
  };

  const handleRebuildAll = () => buildAll(true);

  const handleUpdateAll = () => buildAll(false);

  const handleRemove = async (slug: string) => {
    if (confirm(`Remove project "${slug}"? Graph data will be deleted.`)) {
      await api.removeProject(slug);
      refresh();
    }
  };

  const handleWatch = async (project: ProjectInfo) => {
    if (project.is_watching) {
      await api.stopWatching(project.slug);
    } else {
      await api.startWatching(project.slug);
    }
    refresh();
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
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold text-text-primary">Projects</h1>
          <p className="text-sm text-text-secondary mt-0.5">
            {projects.length} project{projects.length !== 1 ? "s" : ""} registered
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="secondary" size="sm" onClick={handleUpdateAll} disabled={buildingAll || projects.length === 0}>
            <Zap className="w-3.5 h-3.5" />
            Update All
          </Button>
          <Button variant="secondary" size="sm" onClick={handleRebuildAll} disabled={buildingAll || projects.length === 0}>
            <RotateCw className={`w-3.5 h-3.5 ${buildingAll ? "animate-spin" : ""}`} />
            Rebuild All
          </Button>
          <Button size="sm" onClick={handleAdd}>
            <FolderPlus className="w-3.5 h-3.5" />
            Add Project
          </Button>
        </div>
      </header>

      {projects.length === 0 ? (
        <EmptyState onAdd={handleAdd} />
      ) : (
        <div className="grid gap-3">
          {projects.map((p) => (
            <ProjectCard
              key={p.slug}
              project={p}
              isBuilding={building === p.slug}
              buildingPhase={building === p.slug ? buildingPhase : "indexing"}
              onBuild={() => handleBuild(p.slug)}
              onCancel={() => handleCancel(p.slug)}
              onRemove={() => handleRemove(p.slug)}
              onToggleWatch={() => handleWatch(p)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function ProjectCard({
  project,
  isBuilding,
  buildingPhase,
  onBuild,
  onCancel,
  onRemove,
  onToggleWatch,
}: {
  project: ProjectInfo;
  isBuilding: boolean;
  buildingPhase: "indexing" | "embedding";
  onBuild: () => void;
  onCancel: () => void;
  onRemove: () => void;
  onToggleWatch: () => void;
}) {
  const lastBuild = project.last_build
    ? new Date(project.last_build).toLocaleDateString("fr-FR", {
        day: "numeric",
        month: "short",
        hour: "2-digit",
        minute: "2-digit",
      })
    : "Never";

  return (
    <div className="bg-bg-card border border-border rounded-lg p-4 hover:border-border-hover transition-colors duration-150">
      <div className="flex items-start justify-between">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3 className="font-medium text-text-primary truncate">{project.slug}</h3>
            {project.is_watching && (
              <span className="flex items-center gap-1 text-[10px] text-success">
                <span className="w-1.5 h-1.5 bg-success rounded-full animate-pulse" />
                watching
              </span>
            )}
          </div>
          <p className="text-xs text-text-muted mt-1 truncate font-mono">{project.path}</p>
        </div>

        <div className="flex items-center gap-1 ml-3">
          <Button variant="ghost" size="sm" onClick={onToggleWatch} title={project.is_watching ? "Stop watching" : "Start watching"}>
            {project.is_watching ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
          </Button>
          {isBuilding ? (
            <Button variant="ghost" size="sm" onClick={onCancel} title="Cancel build">
              <X className="w-3.5 h-3.5 text-danger" />
            </Button>
          ) : (
            <Button variant="ghost" size="sm" onClick={onBuild} title="Rebuild index">
              <RotateCw className="w-3.5 h-3.5" />
            </Button>
          )}
          <Button variant="ghost" size="sm" onClick={onRemove} title="Remove project">
            <Trash2 className="w-3.5 h-3.5 text-danger" />
          </Button>
        </div>
      </div>

      {isBuilding && (
        <div className="mt-3">
          <ProgressBar label={buildingPhase === "embedding" ? "Embedding..." : "Indexing..."} />
        </div>
      )}

      <div className="mt-3 flex items-center gap-4 text-xs text-text-secondary">
        {project.stats ? (
          <>
            <span><strong className="text-text-primary">{project.stats.symbols}</strong> symbols</span>
            <span><strong className="text-text-primary">{project.stats.edges}</strong> edges</span>
            <span><strong className="text-text-primary">{project.stats.files}</strong> files</span>
          </>
        ) : (
          <span className="text-text-muted">Not indexed yet</span>
        )}
        <span className="ml-auto">Last: {lastBuild}</span>
      </div>

      {project.languages.length > 0 && (
        <div className="mt-2.5 flex flex-wrap gap-1.5">
          {project.languages.map((lang) => (
            <Badge key={lang}>{lang}</Badge>
          ))}
        </div>
      )}
    </div>
  );
}

function EmptyState({ onAdd }: { onAdd: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center py-20 text-center">
      <div className="w-12 h-12 rounded-full bg-accent/10 flex items-center justify-center mb-4">
        <FolderPlus className="w-6 h-6 text-accent" />
      </div>
      <h2 className="text-base font-medium text-text-primary">No projects yet</h2>
      <p className="text-sm text-text-secondary mt-1 max-w-xs">
        Add a project folder to start building its code intelligence graph.
      </p>
      <Button className="mt-4" onClick={onAdd}>
        <FolderPlus className="w-4 h-4" />
        Add Your First Project
      </Button>
    </div>
  );
}
