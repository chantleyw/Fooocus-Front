import type { ReactNode } from "react";

export function ProgressBar({
  value,
  indeterminate = false,
}: {
  /** 0 to 1. Ignored when indeterminate. */
  value: number;
  indeterminate?: boolean;
}) {
  const percent = Math.round(Math.min(1, Math.max(0, value)) * 100);
  return (
    <div className={`progress${indeterminate ? " indeterminate" : ""}`}>
      <div className="progress-fill" style={{ width: `${percent}%` }} />
    </div>
  );
}

export function Chip({
  children,
  tone = "default",
}: {
  children: ReactNode;
  tone?: "default" | "accent" | "success" | "warning" | "danger";
}) {
  return <span className={`chip${tone === "default" ? "" : ` ${tone}`}`}>{children}</span>;
}

export function EmptyState({
  icon,
  title,
  children,
  action,
}: {
  icon: ReactNode;
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="empty">
      <div className="empty-icon">{icon}</div>
      <div>
        <div className="empty-title">{title}</div>
        {children && <div style={{ marginTop: 3, maxWidth: 420 }}>{children}</div>}
      </div>
      {action}
    </div>
  );
}

export function Banner({
  tone = "info",
  icon,
  children,
}: {
  tone?: "info" | "warning" | "danger";
  icon?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className={`banner ${tone}`}>
      {icon && <span style={{ flexShrink: 0, marginTop: 1 }}>{icon}</span>}
      <div>{children}</div>
    </div>
  );
}

export function ScreenHeader({
  title,
  subtitle,
  actions,
}: {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
}) {
  return (
    <header className="screen-header">
      <div style={{ minWidth: 0 }}>
        <h1 className="screen-title">{title}</h1>
        {subtitle && <p className="screen-subtitle">{subtitle}</p>}
      </div>
      {actions && <div className="screen-actions">{actions}</div>}
    </header>
  );
}
