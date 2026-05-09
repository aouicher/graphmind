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

export interface UpdateInfo {
  current: string;
  latest: string;
  update_available: boolean;
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

export interface EmbeddingSettings {
  mode: string;
  model: string | null;
  openai_base_url: string | null;
  openai_key: string | null;
  voyage_key: string | null;
}

export interface EmbeddingSettingsInput {
  mode: string;
  model: string | null;
  openai_base_url: string | null;
  openai_key: string | null;
  voyage_key: string | null;
}

export interface EmbeddingSettingsResult {
  projects_needing_embedding: string[];
}

export interface AppUpdateInfo {
  update_available: boolean;
  current_version: string;
  new_version: string | null;
}

export interface SetupStatus {
  outdated: boolean;
  local_version: number;
  expected_version: number;
}

export interface Announcement {
  id: string;
  message: string;
  level: "info" | "warning" | "breaking";
  url: string | null;
}

export const api = {
  listProjects: () => invoke<ProjectInfo[]>("list_projects"),
  addProject: (path: string) => invoke<ProjectInfo>("add_project", { path }),
  removeProject: (slug: string) => invoke<boolean>("remove_project", { slug }),
  getProjectStatus: (slug: string) => invoke<ProjectInfo>("get_project_status", { slug }),
  buildProject: (slug: string, full: boolean) => invoke<void>("build_project", { slug, full }),
  buildAllProjects: (full: boolean) => invoke<void>("build_all_projects", { full }),
  cancelBuild: (slug: string) => invoke<void>("cancel_build", { slug }),
  startWatching: (slug: string) => invoke<void>("start_watching", { slug }),
  startWatchingAll: () => invoke<number>("start_watching_all"),
  stopWatching: (slug: string) => invoke<void>("stop_watching", { slug }),
  stopWatchingAll: () => invoke<void>("stop_watching_all"),
  getWatchStatus: () => invoke<Record<string, boolean>>("get_watch_status"),
  detectClients: () => invoke<AiClient[]>("detect_clients"),
  installMcp: (clientId: string) => invoke<void>("install_mcp_for_client", { clientId }),
  uninstallMcp: (clientId: string) => invoke<void>("uninstall_mcp_for_client", { clientId }),
  checkCliInstalled: () => invoke<CliStatus>("check_cli_installed"),
  installCli: () => invoke<CliStatus>("install_cli"),
  ensureCliInPath: () => invoke<string>("ensure_cli_in_path"),
  checkCliUpdate: () => invoke<UpdateInfo>("check_cli_update"),
  updateCli: () => invoke<CliStatus>("update_cli"),
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
  getGitHookStatus: (slug?: string) => invoke<boolean>("get_git_hook_status", { slug: slug || null }),
  installGitHook: (slug?: string) => invoke<void>("install_git_hook", { slug: slug || null }),
  uninstallGitHook: (slug?: string) => invoke<void>("uninstall_git_hook", { slug: slug || null }),
  getSkillStatus: () => invoke<boolean>("get_skill_status"),
  installSkill: () => invoke<void>("install_skill"),
  getClaudeMdStatus: () => invoke<boolean>("get_claude_md_status"),
  getEmbeddingSettings: () => invoke<EmbeddingSettings>("get_embedding_settings"),
  setEmbeddingSettings: (settings: EmbeddingSettingsInput) => invoke<EmbeddingSettingsResult>("set_embedding_settings", { settings }),
  embedProjects: (slugs: string[]) => invoke<void>("embed_projects", { slugs }),
  checkAppUpdate: () => invoke<AppUpdateInfo>("check_app_update"),
  installAppUpdate: () => invoke<string>("install_app_update"),
  checkSetupStatus: () => invoke<SetupStatus>("check_setup_status"),
  runSetup: () => invoke<string>("run_setup"),
  checkAnnouncements: () => invoke<Announcement[]>("check_announcements"),
  dismissAnnouncement: (id: string) => invoke<void>("dismiss_announcement", { id }),
};
