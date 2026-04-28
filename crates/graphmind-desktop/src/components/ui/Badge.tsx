interface BadgeProps {
  children: React.ReactNode;
  color?: string;
}

const langColors: Record<string, string> = {
  rust: "bg-orange-500/15 text-orange-400",
  typescript: "bg-blue-500/15 text-blue-400",
  javascript: "bg-yellow-500/15 text-yellow-400",
  python: "bg-green-500/15 text-green-400",
  go: "bg-cyan-500/15 text-cyan-400",
  ruby: "bg-red-500/15 text-red-400",
  markdown: "bg-gray-500/15 text-gray-400",
};

export function Badge({ children, color }: BadgeProps) {
  const lang = typeof children === "string" ? children.toLowerCase() : "";
  const colorClass = color || langColors[lang] || "bg-gray-500/15 text-gray-400";

  return (
    <span
      className={`inline-flex items-center px-2 py-0.5 rounded text-[11px] font-medium ${colorClass}`}
    >
      {children}
    </span>
  );
}
