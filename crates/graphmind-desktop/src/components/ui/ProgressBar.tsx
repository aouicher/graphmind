import { motion } from "framer-motion";

interface ProgressBarProps {
  indeterminate?: boolean;
  label?: string;
}

export function ProgressBar({ indeterminate = true, label }: ProgressBarProps) {
  return (
    <div className="space-y-1.5">
      {label && (
        <p className="text-xs text-text-secondary">{label}</p>
      )}
      <div className="h-1.5 bg-bg-card rounded-full overflow-hidden">
        {indeterminate ? (
          <motion.div
            className="h-full bg-accent rounded-full w-1/3"
            animate={{ x: ["-100%", "400%"] }}
            transition={{ repeat: Infinity, duration: 1.2, ease: "easeInOut" }}
          />
        ) : (
          <div className="h-full bg-accent rounded-full w-full" />
        )}
      </div>
    </div>
  );
}
