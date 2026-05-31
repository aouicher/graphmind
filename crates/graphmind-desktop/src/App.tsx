import { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { listen } from "@tauri-apps/api/event";
import { Sidebar } from "./components/layout/Sidebar";
import { Projects } from "./pages/Projects";
import { Integrations } from "./pages/Integrations";
import { Settings } from "./pages/Settings";
import { Setup } from "./pages/Setup";
import { api, AppUpdateInfo, Announcement } from "./lib/tauri";
import { ArrowUp, X, Loader2, RefreshCw, AlertTriangle, Info } from "lucide-react";

type Page = "projects" | "integrations" | "settings";

const ONBOARDING_KEY = "graphmind_onboarding_done";

function UpdateBanner({
  info,
  onDismiss,
}: {
  info: AppUpdateInfo;
  onDismiss: () => void;
}) {
  const [updating, setUpdating] = useState(false);
  const [cliUpdating, setCliUpdating] = useState(false);
  const [done, setDone] = useState(false);

  const handleUpdate = async () => {
    setUpdating(true);
    try {
      await api.installAppUpdate();
      setDone(true);
      // Also update CLI — fire-and-forget, must not block or prevent app update
      setCliUpdating(true);
      api.updateCli()
        .catch(() => {})
        .finally(() => setCliUpdating(false));
    } catch (e) {
      console.error(e);
    }
    setUpdating(false);
  };

  if (done) {
    return (
      <div className="flex items-center justify-between px-4 py-2 bg-success/10 border-b border-success/20 text-xs">
        <span className="text-success font-medium flex items-center gap-1.5">
          Updated to v{info.new_version}. Restart to apply.
          {cliUpdating && (
            <span className="text-text-muted font-normal flex items-center gap-1">
              <Loader2 className="w-3 h-3 animate-spin" />
              Updating CLI...
            </span>
          )}
        </span>
        <button onClick={onDismiss} className="text-success/60 hover:text-success">
          <X className="w-3.5 h-3.5" />
        </button>
      </div>
    );
  }

  return (
    <div className="flex items-center justify-between px-4 py-2 bg-accent/5 border-b border-accent/10 text-xs">
      <span className="text-text-secondary">
        <span className="font-medium text-accent">v{info.new_version}</span> available
        (current: v{info.current_version})
      </span>
      <div className="flex items-center gap-2">
        <button
          onClick={handleUpdate}
          disabled={updating}
          className="flex items-center gap-1 px-2 py-1 rounded bg-accent text-white font-medium hover:bg-accent-hover disabled:opacity-50 transition-colors"
        >
          {updating ? (
            <Loader2 className="w-3 h-3 animate-spin" />
          ) : (
            <ArrowUp className="w-3 h-3" />
          )}
          {updating ? "Updating..." : "Update"}
        </button>
        <button onClick={onDismiss} className="text-text-muted hover:text-text-secondary">
          <X className="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  );
}

function SetupBanner({ onDismiss }: { onDismiss: () => void }) {
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(false);

  const handleSetup = async () => {
    setRunning(true);
    try {
      await api.runSetup();
      setDone(true);
    } catch (e) {
      console.error(e);
    }
    setRunning(false);
  };

  if (done) {
    return (
      <div className="flex items-center justify-between px-4 py-2 bg-success/10 border-b border-success/20 text-xs">
        <span className="text-success font-medium">Hooks & skills updated.</span>
        <button onClick={onDismiss} className="text-success/60 hover:text-success">
          <X className="w-3.5 h-3.5" />
        </button>
      </div>
    );
  }

  return (
    <div className="flex items-center justify-between px-4 py-2 bg-warning/10 border-b border-warning/20 text-xs">
      <span className="text-text-secondary">
        <AlertTriangle className="w-3 h-3 inline mr-1" />
        Hooks/skills outdated. Update to get the latest features.
      </span>
      <div className="flex items-center gap-2">
        <button
          onClick={handleSetup}
          disabled={running}
          className="flex items-center gap-1 px-2 py-1 rounded bg-warning text-white font-medium hover:bg-warning/80 disabled:opacity-50 transition-colors"
        >
          {running ? <Loader2 className="w-3 h-3 animate-spin" /> : <RefreshCw className="w-3 h-3" />}
          {running ? "Updating..." : "Update"}
        </button>
        <button onClick={onDismiss} className="text-text-muted hover:text-text-secondary">
          <X className="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  );
}

function AnnouncementBanner({
  announcement,
  onDismiss,
}: {
  announcement: Announcement;
  onDismiss: () => void;
}) {
  const colors = {
    breaking: "bg-red-500/10 border-red-500/20 text-red-400",
    warning: "bg-warning/10 border-warning/20 text-warning",
    info: "bg-accent/5 border-accent/10 text-accent",
  };
  const colorClass = colors[announcement.level] || colors.info;

  return (
    <div className={`flex items-center justify-between px-4 py-2 border-b text-xs ${colorClass}`}>
      <span className="flex items-center gap-1">
        <Info className="w-3 h-3" />
        {announcement.message}
        {announcement.url && (
          <a href={announcement.url} target="_blank" rel="noreferrer" className="underline ml-1 opacity-80">
            Learn more
          </a>
        )}
      </span>
      <button onClick={onDismiss} className="opacity-60 hover:opacity-100">
        <X className="w-3.5 h-3.5" />
      </button>
    </div>
  );
}

export default function App() {
  const [page, setPage] = useState<Page>("projects");
  const [needsSetup, setNeedsSetup] = useState<boolean | null>(null);
  const [updateInfo, setUpdateInfo] = useState<AppUpdateInfo | null>(null);
  const [setupOutdated, setSetupOutdated] = useState(false);
  const [announcements, setAnnouncements] = useState<Announcement[]>([]);

  useEffect(() => {
    const done = localStorage.getItem(ONBOARDING_KEY);
    if (done === "true") {
      setNeedsSetup(false);
    } else {
      setNeedsSetup(true);
    }
  }, []);

  useEffect(() => {
    if (needsSetup === false) {
      api.checkAppUpdate().then((info) => {
        if (info.update_available) setUpdateInfo(info);
      }).catch(() => {});
      api.checkSetupStatus().then((status) => {
        if (status.outdated) setSetupOutdated(true);
      }).catch(() => {});
      api.checkAnnouncements().then(setAnnouncements).catch(() => {});

      // Trigger build-all on startup if enabled in settings
      api.getStartupSettings().then((s) => {
        if (s.build_all_on_startup) api.buildAllProjects(false).catch(() => {});
      }).catch(() => {});

      // Keep listening for explicit startup-build-all events (e.g. future use)
      const unlisten = listen("startup-build-all", () => {
        api.buildAllProjects(false).catch(() => {});
      });
      return () => { unlisten.then((fn) => fn()); };
    }
  }, [needsSetup]);

  if (needsSetup === null) return null;

  if (needsSetup) {
    return (
      <Setup
        onComplete={() => {
          localStorage.setItem(ONBOARDING_KEY, "true");
          setNeedsSetup(false);
        }}
      />
    );
  }

  return (
    <div className="flex h-screen w-screen overflow-hidden">
      <Sidebar activePage={page} onNavigate={setPage} />
      <div className="flex-1 flex flex-col overflow-hidden">
        {updateInfo && (
          <UpdateBanner info={updateInfo} onDismiss={() => setUpdateInfo(null)} />
        )}
        {setupOutdated && (
          <SetupBanner onDismiss={() => setSetupOutdated(false)} />
        )}
        {announcements.map((a) => (
          <AnnouncementBanner
            key={a.id}
            announcement={a}
            onDismiss={() => {
              api.dismissAnnouncement(a.id);
              setAnnouncements((prev) => prev.filter((x) => x.id !== a.id));
            }}
          />
        ))}
        <main className="flex-1 overflow-y-auto">
          <AnimatePresence mode="wait">
            <motion.div
              key={page}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -8 }}
              transition={{ duration: 0.15 }}
              className="h-full"
            >
              {page === "projects" && <Projects />}
              {page === "integrations" && <Integrations />}
              {page === "settings" && <Settings />}
            </motion.div>
          </AnimatePresence>
        </main>
      </div>
    </div>
  );
}
