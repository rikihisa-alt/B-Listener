import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Alert } from "@/components/ui/Feedback";
import { toMessage } from "@/lib/errors";
import { formatDate, formatDurationJa } from "@/lib/format";
import { getRecoverableMeetings, recoverMeeting } from "@/lib/ipc";
import type { RecoverableMeeting, WavCondition } from "@/types/ipc";

const CONDITION_NOTE: Partial<Record<WavCondition, string>> = {
  "header-outdated": "ファイルの修復が必要です（自動で修復します）",
  "header-broken": "別ファイルとして救出します",
  empty: "録音データが記録されていません",
  missing: "録音ファイルが見つかりません",
};

/**
 * 前回の異常終了で中断された会議の復旧案内。
 *
 * 「録音データを絶対に失わない」方針のため、破棄の選択肢は用意しない。
 */
export function RecoveryNotice({ onRecovered }: { onRecovered: () => void }) {
  const [items, setItems] = useState<RecoverableMeeting[]>([]);
  const [recovering, setRecovering] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setItems(await getRecoverableMeetings());
    } catch (e) {
      setError(toMessage(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (items.length === 0) return null;

  async function recover(meetingId: string) {
    setRecovering(meetingId);
    setError(null);
    try {
      await recoverMeeting(meetingId);
      await load();
      onRecovered();
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setRecovering(null);
    }
  }

  return (
    <div style={{ marginBottom: 24 }}>
      <Card title="中断された会議があります" icon="alert" iconTone="orange">
        <div className="stack gap-16">
          <Alert tone="warn">
            前回アプリが正常に終了しなかったため、録音が中断されたままになっています。
            録音データはディスクに残っているので、下のボタンで保存できます。
          </Alert>
          {error && <Alert tone="danger" title="エラー">{error}</Alert>}

          <ul className="divided-list">
            {items.map((item) => (
              <li key={item.meeting.id}>
                <div className="row row--top gap-16">
                  <div className="grow">
                    <div className="bold">{item.meeting.title}</div>
                    <div className="text-sm text-muted">
                      {formatDate(item.meeting.startedAt ?? item.meeting.createdAt)}
                      {" ・ 録音 "}
                      {formatDurationJa(item.inspection.durationMs)}
                      {CONDITION_NOTE[item.inspection.condition]
                        ? ` ・ ${CONDITION_NOTE[item.inspection.condition]}`
                        : ""}
                    </div>
                    <div className="mono text-faint">{item.inspection.path}</div>
                  </div>
                  <Button
                    variant="primary"
                    icon="download"
                    disabled={!item.canRecover || recovering === item.meeting.id}
                    onClick={() => void recover(item.meeting.id)}
                  >
                    {recovering === item.meeting.id ? "保存中…" : "録音を保存する"}
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        </div>
      </Card>
    </div>
  );
}
