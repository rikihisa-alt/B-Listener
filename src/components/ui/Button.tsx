import type { ButtonHTMLAttributes, ReactNode } from "react";

import { Icon, type IconName } from "./Icon";

type Variant = "default" | "primary" | "danger" | "ghost" | "link";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  /** 「会議開始」「会議終了」など、迷わせてはいけない操作に使う */
  large?: boolean;
  block?: boolean;
  icon?: IconName;
  iconFilled?: boolean;
  children: ReactNode;
}

const VARIANT_CLASS: Record<Variant, string> = {
  default: "",
  primary: "btn--primary",
  danger: "btn--danger",
  ghost: "btn--ghost",
  link: "btn--link",
};

export function Button({
  variant = "default",
  large = false,
  block = false,
  icon,
  iconFilled = false,
  className,
  type = "button",
  children,
  ...rest
}: ButtonProps) {
  const classes = ["btn", VARIANT_CLASS[variant], large ? "btn--large" : "", block ? "btn--block" : "", className ?? ""]
    .filter(Boolean)
    .join(" ");

  return (
    <button type={type} className={classes} {...rest}>
      {icon && <Icon name={icon} size={large ? 19 : 17} filled={iconFilled} />}
      {children}
    </button>
  );
}
