import { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Sidebar } from "./components/layout/Sidebar";
import { Projects } from "./pages/Projects";
import { Integrations } from "./pages/Integrations";
import { Settings } from "./pages/Settings";
import { Setup } from "./pages/Setup";

type Page = "projects" | "integrations" | "settings";

const ONBOARDING_KEY = "graphmind_onboarding_done";

export default function App() {
  const [page, setPage] = useState<Page>("projects");
  const [needsSetup, setNeedsSetup] = useState<boolean | null>(null);

  useEffect(() => {
    const done = localStorage.getItem(ONBOARDING_KEY);
    if (done === "true") {
      setNeedsSetup(false);
    } else {
      setNeedsSetup(true);
    }
  }, []);

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
  );
}
