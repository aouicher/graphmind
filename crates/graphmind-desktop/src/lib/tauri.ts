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
};
