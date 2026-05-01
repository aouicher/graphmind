import { invoke } from "@tauri-apps/api/core";

export interface ProjectInfo {
  slug: string;
  path: string;
  last_build: string | null;
  languages: string[];
  stats: GraphStats | null;
  is_watching: boolean;
}

export interface GraphStats {
  symbols: number;
  edges: number;
  files: number;
}

export interface AiClient {
  id: string;
  name: string;
  icon: string;
  detected: boolean;
  mcp_configured: boolean;
  config_path: string | null;
}

export interface CliStatus {
  installed: boolean;
  path: string | null;
  version: string | null;
}

export interface GraphNode {
  id: string;
  name: string;
  kind: string;
  file: string;
  line_start: number;
  connections: number;
}

export interface GraphEdge {
  source: string;
  target: string;
  kind: string;
}

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
  files: string[];
  kinds: string[];
  languages: string[];
}

export interface ExcludeSettings {
  global: string[];
  project: string[];
}

export const api = {
  listProjects: () => invoke<ProjectInfo[]>("list_projects"),
  addProject: (path: string) => invoke<ProjectInfo>("add_project", { path }),
  removeProject: (slug: string) => invoke<boolean>("remove_project", { slug }),
  getProjectStatus: (slug: string) => invoke<ProjectInfo>("get_project_status", { slug }),
  buildProject: (slug: string, full: boolean) => invoke<void>("build_project", { slug, full }),
  buildAllProjects: (full: boolean) => invoke<void>("build_all_projects", { full }),
  startWatching: (slug: string) => invoke<void>("start_watching", { slug }),
  stopWatching: (slug: string) => invoke<void>("stop_watching", { slug }),
  getWatchStatus: () => invoke<Record<string, boolean>>("get_watch_status"),
  detectClients: () => invoke<AiClient[]>("detect_clients"),
  installMcp: (clientId: string) => invoke<void>("install_mcp_for_client", { clientId }),
  uninstallMcp: (clientId: string) => invoke<void>("uninstall_mcp_for_client", { clientId }),
  checkCliInstalled: () => invoke<CliStatus>("check_cli_installed"),
  installCli: () => invoke<CliStatus>("install_cli"),
  getCliPath: () => invoke<string>("get_cli_path"),
  getGraphData: (slug: string, fileFilter?: string, kindFilter?: string, languageFilter?: string, limit?: number) =>
    invoke<GraphData>("get_graph_data", { slug, fileFilter: fileFilter || null, kindFilter: kindFilter || null, languageFilter: languageFilter || null, limit: limit || null }),
  getExcludes: (slug?: string) => invoke<ExcludeSettings>("get_excludes", { slug: slug || null }),
  setGlobalExcludes: (excludes: string[]) => invoke<void>("set_global_excludes", { excludes }),
  setProjectExcludes: (slug: string, excludes: string[]) => invoke<void>("set_project_excludes", { slug, excludes }),
  getAppVersion: () => invoke<string>("get_app_version"),
  getHookStatus: () => invoke<boolean>("get_hook_status"),
  installClaudeHook: () => invoke<void>("install_claude_hook"),
  uninstallClaudeHook: () => invoke<void>("uninstall_claude_hook"),
};
