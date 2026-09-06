import type { ReactNode } from "react";

import { Icon, type IconName } from "./Icon";

type Tone = "neutral" | "ok" | "warn" | "danger" | "info";

const BADGE_CLASS: Record<Tone, string> = {
  neutral: "",
  ok: "badge--ok",
  warn: "badge--warn",
  danger: "badge--danger",
  info: "badge--info",
};

export function Badge({
  tone = "neutral",
  icon,
  children,
}: {
  tone?: Tone;
  icon?: IconName;
  children: ReactNode;
}) {
  return (
    <span className={["badge", BADGE_CLASS[tone]].filter(Boolean).join(" ")}>
      {icon && <Icon name={icon} size={13} />}
      {children}
    </span>
  );
}

const ALERT_CLASS: Record<Tone, string> = {
  neutral: "",
  ok: "alert--ok",
  warn: "alert--warn",
  danger: "alert--error",
  info: "",
};

const ALERT_ICON: Record<Tone, IconName> = {
  neutral: "help",
  ok: "check-circle",
  warn: "alert",
  danger: "alert",
  info: "sparkles",
};

interface AlertProps {
  tone?: Tone;
  title?: string;
  children?: ReactNode;
}

export function Alert({ tone = "neutral", title, children }: AlertProps) {
  return (
    <div className={["alert", ALERT_CLASS[tone]].filter(Boolean).join(" ")} role="status">
      <Icon name={ALERT_ICON[tone]} size={18} />
      <div className="alert__body">
        {title && <div className="alert__title">{title}</div>}
        {children}
      </div>
    </div>
  );
}

/** エラーは握り潰さず、必ず画面に出す。 */
export function ErrorBanner({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <Alert tone="danger" title="エラー">
      {message}
    </Alert>
  );
}

export function EmptyState({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <div className="empty__title">{title}</div>
      {children}
    </div>
  );
}
