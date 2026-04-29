import { useCallback, useState } from "react";
import { FolderPlus, RefreshCw, Eye, EyeOff, Trash2, RotateCcw } from "lucide-react";
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

  useTauriEvent<string>("indexing-complete", useCallback(() => {
    setBuilding(null);
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
          <Button variant="secondary" size="sm" onClick={refresh}>
            <RefreshCw className="w-3.5 h-3.5" />
            Refresh
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
              onBuild={() => handleBuild(p.slug)}
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
  onBuild,
  onRemove,
  onToggleWatch,
}: {
  project: ProjectInfo;
  isBuilding: boolean;
  onBuild: () => void;
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
          <Button variant="ghost" size="sm" onClick={onBuild} disabled={isBuilding} title="Rebuild index">
            <RotateCcw className={`w-3.5 h-3.5 ${isBuilding ? "animate-spin" : ""}`} />
          </Button>
          <Button variant="ghost" size="sm" onClick={onRemove} title="Remove project">
            <Trash2 className="w-3.5 h-3.5 text-danger" />
          </Button>
        </div>
      </div>

      {isBuilding && (
        <div className="mt-3">
          <ProgressBar label="Indexing..." />
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
