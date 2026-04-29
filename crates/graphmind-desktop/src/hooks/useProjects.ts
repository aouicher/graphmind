import { useCallback, useEffect, useState } from "react";
import { api, ProjectInfo } from "../lib/tauri";

export function useProjects() {
  const [projects, setProjects] = useState<ProjectInfo[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const data = await api.listProjects();
      data.sort((a, b) => a.slug.localeCompare(b.slug));
      setProjects(data);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { projects, loading, refresh };
}
