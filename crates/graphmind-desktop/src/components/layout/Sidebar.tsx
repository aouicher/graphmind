import { FolderGit2, Plug, GitGraph, Network, SlidersHorizontal } from "lucide-react";

interface SidebarProps {
  activePage: string;
  onNavigate: (page: "projects" | "integrations" | "graph" | "settings") => void;
}

const navItems = [
  { id: "projects" as const, icon: FolderGit2, label: "Projects" },
  { id: "graph" as const, icon: Network, label: "Graph" },
  { id: "integrations" as const, icon: Plug, label: "Integrations" },
  { id: "settings" as const, icon: SlidersHorizontal, label: "Settings" },
];

export function Sidebar({ activePage, onNavigate }: SidebarProps) {
  return (
    <aside className="w-56 h-full bg-bg-sidebar border-r border-border flex flex-col">
      <div className="h-12 flex items-center gap-2 pl-20 pr-4 border-b border-border drag-region">
        <GitGraph className="w-5 h-5 text-accent" />
        <span className="font-semibold text-sm text-text-primary">GraphMind</span>
      </div>

      <nav className="flex-1 py-3 px-2 space-y-1">
        {navItems.map((item) => {
          const isActive = activePage === item.id;
          return (
            <button
              key={item.id}
              onClick={() => onNavigate(item.id)}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-md text-sm transition-colors duration-150 ${
                isActive
                  ? "bg-accent/10 text-accent"
                  : "text-text-secondary hover:text-text-primary hover:bg-bg-card"
              }`}
            >
              <item.icon className="w-4 h-4" />
              <span>{item.label}</span>
            </button>
          );
        })}
      </nav>

      <div className="p-3 border-t border-border">
        <p className="text-[10px] text-text-muted text-center">
          GraphMind v0.2.33
        </p>
      </div>
    </aside>
  );
}
