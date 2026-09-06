import { useCallback, useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";

import { Button } from "@/components/ui/Button";
import { EmptyState, ErrorBanner } from "@/components/ui/Feedback";
import { Icon, IconBadge, type IconName } from "@/components/ui/Icon";
import { toMessage } from "@/lib/errors";
import { getHomeStats, listMeetings } from "@/lib/ipc";
import type { HomeStats, MeetingListItem } from "@/types/ipc";

import { MeetingRow } from "./MeetingRow";
import { RecoveryNotice } from "./RecoveryNotice";
import "./home.css";

/** 表示する「最近の会議」の件数。 */
const RECENT_COUNT = 4;

function greeting(): string {
  const hour = new Date().getHours();
  if (hour < 5) return "こんばんは";
  if (hour < 11) return "おはようございます";
  if (hour < 18) return "こんにちは";
  return "こんばんは";
}

export function HomePage() {
  const navigate = useNavigate();
  const [meetings, setMeetings] = useState<MeetingListItem[] | null>(null);
  const [stats, setStats] = useState<HomeStats | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setError(null);
      const [list, s] = await Promise.all([listMeetings(), getHomeStats()]);
      setMeetings(list);
      setStats(s);
    } catch (e) {
      setError(toMessage(e));
      setMeetings([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const recent = (meetings ?? []).slice(0, RECENT_COUNT);

  return (
    <div className="page">
      <div className="page__head">
        <div className="page__head-text">
          <h1>{greeting()}</h1>
          <p className="page__desc">AIが会議の内容を整理し、重要なポイントをまとめます。</p>
        </div>
        <Button variant="primary" large icon="plus-circle" onClick={() => navigate("/meetings/new")}>
          新しい会議
        </Button>
      </div>

      <ErrorBanner message={error} />

      <RecoveryNotice onRecovered={() => void load()} />

      <div className="stat-grid">
        <StatCard icon="users" tone="blue" label="今月の会議" value={stats?.meetingsThisMonth} />
        <StatCard icon="document" tone="orange" label="未処理" value={stats?.unprocessed} />
        <StatCard icon="waveform" tone="green" label="保存済み音声" value={stats?.savedAudio} />
      </div>

      <div className="section-head">
        <h2 className="grow" style={{ fontSize: 19 }}>
          最近の会議
        </h2>
        {(meetings?.length ?? 0) > RECENT_COUNT && (
          <Link to="/meetings" className="btn btn--link">
            すべての会議を見る
            <Icon name="arrow-right" size={15} />
          </Link>
        )}
      </div>

      {meetings === null && <div className="empty">読み込み中…</div>}

      {meetings !== null && recent.length === 0 && !error && (
        <div className="card">
          <EmptyState title="まだ会議がありません">
            <p>「新しい会議」から会議を作成してください。</p>
          </EmptyState>
        </div>
      )}

      {recent.length > 0 && (
        <ul className="meeting-list">
          {recent.map((m) => (
            <li key={m.id}>
              <MeetingRow meeting={m} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function StatCard({
  icon,
  tone,
  label,
  value,
}: {
  icon: IconName;
  tone: "blue" | "orange" | "green";
  label: string;
  value: number | undefined;
}) {
  return (
    <div className="stat-card">
      <IconBadge name={icon} tone={tone} />
      <div>
        <div className="stat-card__label">{label}</div>
        <div className="stat-card__value">
          {value ?? "—"}
          <span className="stat-card__unit">件</span>
        </div>
      </div>
    </div>
  );
}
