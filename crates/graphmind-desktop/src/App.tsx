import { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Sidebar } from "./components/layout/Sidebar";
import { Projects } from "./pages/Projects";
import { Integrations } from "./pages/Integrations";
import { Settings } from "./pages/Settings";
import { Setup } from "./pages/Setup";
import { api, UpdateInfo } from "./lib/tauri";
import { ArrowUp, X, Loader2 } from "lucide-react";

type Page = "projects" | "integrations" | "settings";

const ONBOARDING_KEY = "graphmind_onboarding_done";

function UpdateBanner({
  info,
  onDismiss,
}: {
  info: UpdateInfo;
  onDismiss: () => void;
}) {
  const [updating, setUpdating] = useState(false);
  const [done, setDone] = useState(false);

  const handleUpdate = async () => {
    setUpdating(true);
    try {
      await api.updateCli();
      setDone(true);
    } catch (e) {
      console.error(e);
    }
    setUpdating(false);
  };

  if (done) {
    return (
      <div className="flex items-center justify-between px-4 py-2 bg-success/10 border-b border-success/20 text-xs">
        <span className="text-success font-medium">
          Updated to v{info.latest}. Restart to apply.
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
        <span className="font-medium text-accent">v{info.latest}</span> available
        (current: v{info.current})
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

export default function App() {
  const [page, setPage] = useState<Page>("projects");
  const [needsSetup, setNeedsSetup] = useState<boolean | null>(null);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);

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
      api.checkCliUpdate().then((info) => {
        if (info.update_available) setUpdateInfo(info);
      }).catch(() => {});
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
