import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";

import { Button } from "@/components/ui/Button";
import { EmptyState, ErrorBanner } from "@/components/ui/Feedback";
import { toMessage } from "@/lib/errors";
import { listMeetings } from "@/lib/ipc";
import type { MeetingListItem } from "@/types/ipc";

import { MeetingRow } from "./MeetingRow";
import "./home.css";

/** 過去の会議をすべて一覧する。 */
export function MeetingListPage() {
  const navigate = useNavigate();
  const [meetings, setMeetings] = useState<MeetingListItem[] | null>(null);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setError(null);
      setMeetings(await listMeetings());
    } catch (e) {
      setError(toMessage(e));
      setMeetings([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const filtered = useMemo(() => {
    const q = query.trim();
    if (!q || !meetings) return meetings ?? [];
    return meetings.filter((m) => m.title.includes(q));
  }, [meetings, query]);

  return (
    <div className="page">
      <div className="page__head">
        <div className="page__head-text">
          <h1>会議一覧</h1>
          <p className="page__desc">
            過去の会議の音声・議事録・AIまとめを確認できます。
          </p>
        </div>
        <Button variant="primary" icon="plus-circle" onClick={() => navigate("/meetings/new")}>
          新しい会議
        </Button>
      </div>

      <ErrorBanner message={error} />

      <div style={{ marginBottom: 16, maxWidth: 340 }}>
        <input
          className="input"
          type="search"
          value={query}
          placeholder="会議名で絞り込む"
          aria-label="会議名で絞り込む"
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>

      {meetings === null && <div className="empty">読み込み中…</div>}

      {meetings !== null && filtered.length === 0 && (
        <div className="card">
          <EmptyState title={query ? "該当する会議がありません" : "まだ会議がありません"}>
            {!query && <p>「新しい会議」から会議を作成してください。</p>}
          </EmptyState>
        </div>
      )}

      {filtered.length > 0 && (
        <ul className="meeting-list">
          {filtered.map((m) => (
            <li key={m.id}>
              <MeetingRow meeting={m} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
