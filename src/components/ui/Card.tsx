import type { ReactNode } from "react";

import { IconBadge, type IconName } from "./Icon";

interface CardProps {
  title?: ReactNode;
  /** 見出し左の丸アイコン */
  icon?: IconName;
  iconTone?: "blue" | "orange" | "green" | "purple" | "red";
  subtitle?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
  bodyClassName?: string;
}

export function Card({
  title,
  icon,
  iconTone = "blue",
  subtitle,
  actions,
  children,
  className,
  bodyClassName,
}: CardProps) {
  const hasHead = Boolean(title || actions);
  return (
    <section className={["card", className ?? ""].filter(Boolean).join(" ")}>
      {hasHead && (
        <header className="card__head">
          {icon && <IconBadge name={icon} tone={iconTone} />}
          {title && (
            <div className="grow">
              <h2>{title}</h2>
              {subtitle && <div className="text-sm text-muted">{subtitle}</div>}
            </div>
          )}
          {actions}
        </header>
      )}
      <div className={["card__body", bodyClassName ?? ""].filter(Boolean).join(" ")}>{children}</div>
    </section>
  );
}
