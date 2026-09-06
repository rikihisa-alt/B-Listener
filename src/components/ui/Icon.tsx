/**
 * インライン SVG アイコン。
 *
 * 外部フォントやアイコンライブラリを読み込まない（外部通信を発生させないため）。
 * すべて 24x24 のストロークアイコンで統一する。
 */

export type IconName =
  | "home"
  | "plus-circle"
  | "document"
  | "settings"
  | "users"
  | "waveform"
  | "calendar"
  | "clock"
  | "play"
  | "download"
  | "folder"
  | "sparkles"
  | "check-circle"
  | "alert"
  | "help"
  | "target"
  | "list"
  | "arrow-left"
  | "arrow-right"
  | "pause"
  | "stop"
  | "close"
  | "chevron-down"
  | "search"
  | "trash"
  | "refresh"
  | "mic";

const PATHS: Record<IconName, string> = {
  home: "M3 10.5 12 3l9 7.5M5.25 9.75V20a1 1 0 0 0 1 1h3.5v-5.5h4.5V21h3.5a1 1 0 0 0 1-1V9.75",
  "plus-circle": "M12 8v8M8 12h8M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z",
  document:
    "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8l-5-5Zm0 0v5h5M9 13h6M9 17h4",
  settings:
    "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6 1.65 1.65 0 0 0 10 3.09V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9v.09a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z",
  users:
    "M17 20v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M9.5 10a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7ZM22 20v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75",
  waveform: "M4 10v4M8 6v12M12 3v18M16 7v10M20 10v4",
  calendar:
    "M8 3v4M16 3v4M4 9h16M5 5h14a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1Z",
  clock: "M12 7v5l3 2M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z",
  play: "M7 4.5v15l13-7.5-13-7.5Z",
  download: "M12 3v13M7 11l5 5 5-5M4 20h16",
  folder:
    "M3 7a2 2 0 0 1 2-2h4l2 2.5h8a2 2 0 0 1 2 2V18a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z",
  sparkles:
    "M12 3l1.9 4.6L18.5 9.5l-4.6 1.9L12 16l-1.9-4.6L5.5 9.5l4.6-1.9L12 3ZM18.5 15l.9 2.1 2.1.9-2.1.9-.9 2.1-.9-2.1-2.1-.9 2.1-.9.9-2.1Z",
  "check-circle": "M8.5 12.5l2.5 2.5 4.5-5M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z",
  alert: "M12 8.5v4.5M12 16.5h.01M10.3 3.9 2.6 17.4A2 2 0 0 0 4.3 20.5h15.4a2 2 0 0 0 1.7-3.1L13.7 3.9a2 2 0 0 0-3.4 0Z",
  help: "M9.8 9.3a2.3 2.3 0 1 1 3.1 2.1c-.6.3-.9.9-.9 1.6v.4M12 17h.01M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z",
  target:
    "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18ZM12 16.5a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9ZM12 13.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z",
  list: "M8 6h13M8 12h13M8 18h13M3.5 6h.01M3.5 12h.01M3.5 18h.01",
  "arrow-left": "M19 12H5M11 18l-6-6 6-6",
  "arrow-right": "M5 12h14M13 6l6 6-6 6",
  pause: "M9 5v14M15 5v14",
  stop: "M6.5 6.5h11v11h-11z",
  close: "M6 6l12 12M18 6 6 18",
  "chevron-down": "M6 9.5l6 6 6-6",
  search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14ZM20 20l-4-4",
  trash: "M4 7h16M10 11v6M14 11v6M6 7l1 13a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1l1-13M9 7V4h6v3",
  refresh:
    "M20 11a8 8 0 0 0-13.7-5.7L3 8M4 13a8 8 0 0 0 13.7 5.7L21 16M3 4v4h4M21 20v-4h-4",
  mic: "M12 15a3 3 0 0 0 3-3V6a3 3 0 0 0-6 0v6a3 3 0 0 0 3 3ZM6 11v1a6 6 0 0 0 12 0v-1M12 18v3",
};

interface IconProps {
  name: IconName;
  size?: number;
  /** 塗りつぶしアイコンにする（play など） */
  filled?: boolean;
  className?: string;
}

export function Icon({ name, size = 18, filled = false, className }: IconProps) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke={filled ? "none" : "currentColor"}
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      style={{ flex: "0 0 auto", display: "block" }}
    >
      <path d={PATHS[name]} />
    </svg>
  );
}

/** 色付きの丸背景にアイコンを載せる（統計カードや見出し用） */
export function IconBadge({
  name,
  tone = "blue",
  size = 20,
}: {
  name: IconName;
  tone?: "blue" | "orange" | "green" | "purple" | "red";
  size?: number;
}) {
  return (
    <span className={`icon-badge icon-badge--${tone}`}>
      <Icon name={name} size={size} />
    </span>
  );
}
