import { ButtonHTMLAttributes } from "react";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md";
}

const variants = {
  primary: "bg-accent hover:bg-accent-hover text-white",
  secondary: "bg-bg-card hover:bg-bg-card-hover text-text-primary border border-border",
  ghost: "hover:bg-bg-card text-text-secondary hover:text-text-primary",
  danger: "bg-danger/10 hover:bg-danger/20 text-danger",
};

const sizes = {
  sm: "px-2.5 py-1.5 text-xs",
  md: "px-3.5 py-2 text-sm",
};

export function Button({
  variant = "primary",
  size = "md",
  className = "",
  children,
  ...props
}: ButtonProps) {
  return (
    <button
      className={`inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition-colors duration-150 disabled:opacity-50 disabled:cursor-not-allowed ${variants[variant]} ${sizes[size]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
}
