import type { ReactNode } from "react";

import { Icon, type IconName } from "@/components/ui/Icon";

type Tone = "blue" | "green" | "orange" | "purple" | "plain";

/**
 * AI分析パネルの 1 セクション。
 *
 * Phase 8 でリアルタイム分析を実装する際、ここへ構造化データを流し込む。
 * 「発言されていない内容は表示しない」方針のため、
 * 該当データが無いセクションは空表示にして、推測で埋めない。
 */
export function AnalysisSection({
  title,
  icon,
  tone,
  items,
  emptyText,
}: {
  title: string;
  icon: IconName;
  tone: Tone;
  items: readonly string[];
  emptyText: string;
}) {
  return (
    <section className="analysis-section">
      <h3 className="analysis-section__head">
        <span className={`icon-${tone === "plain" ? "blue" : tone}`}>
          <Icon name={icon} size={16} />
        </span>
        {title}
      </h3>
      <div className={`analysis-section__body ${items.length > 0 ? `tone-${tone}` : "tone-plain"}`}>
        {items.length === 0 ? (
          emptyText
        ) : (
          <ul>
            {items.map((item, i) => (
              <li key={i}>{item}</li>
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}

export function AnalysisPanel({ children }: { children: ReactNode }) {
  return <>{children}</>;
}
