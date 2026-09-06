import type { ReactNode } from "react";
import { Link, useLocation } from "react-router-dom";

import { Icon, type IconName } from "@/components/ui/Icon";

interface NavEntry {
  to: string;
  label: string;
  icon: IconName;
  /** この項目を選択中として表示するか。前方一致だと "/meetings/new" が
      「会議一覧」も光らせてしまうため、項目ごとに条件を持たせる。 */
  isActive: (pathname: string) => boolean;
}

const NAV: readonly NavEntry[] = [
  { to: "/", label: "ホーム", icon: "home", isActive: (p) => p === "/" },
  {
    to: "/meetings/new",
    label: "新しい会議",
    icon: "plus-circle",
    isActive: (p) => p === "/meetings/new",
  },
  {
    to: "/meetings",
    label: "会議一覧",
    icon: "document",
    // 会議詳細も「会議一覧」の配下として扱う
    isActive: (p) => p.startsWith("/meetings") && p !== "/meetings/new",
  },
  { to: "/settings", label: "設定", icon: "settings", isActive: (p) => p.startsWith("/settings") },
];

/** サイドバー + 本文の 2 カラムレイアウト。 */
export function AppLayout({ children }: { children: ReactNode }) {
  const { pathname } = useLocation();

  return (
    <div className="app">
      <nav className="sidebar" aria-label="メインメニュー">
        <div className="sidebar__brand">
          <span className="sidebar__logo">
            <Icon name="waveform" size={17} />
          </span>
          <span className="sidebar__name">AI議事録アプリ</span>
        </div>

        <div className="sidebar__nav">
          {NAV.map((item) => (
            <Link
              key={item.to}
              to={item.to}
              className={item.isActive(pathname) ? "nav-item nav-item--active" : "nav-item"}
              aria-current={item.isActive(pathname) ? "page" : undefined}
            >
              <Icon name={item.icon} size={19} />
              {item.label}
            </Link>
          ))}
        </div>

        <div className="sidebar__foot">
          AIで、
          <br />
          会議をもっとシンプルに。
        </div>
      </nav>

      <main className="main">{children}</main>
    </div>
  );
}
