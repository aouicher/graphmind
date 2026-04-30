import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Sidebar } from "./components/layout/Sidebar";
import { Projects } from "./pages/Projects";
import { Integrations } from "./pages/Integrations";
import { Settings } from "./pages/Settings";

type Page = "projects" | "integrations" | "settings";

export default function App() {
  const [page, setPage] = useState<Page>("projects");

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
