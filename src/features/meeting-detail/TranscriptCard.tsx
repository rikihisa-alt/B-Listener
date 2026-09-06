import { useEffect, useState } from "react";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/ui/Feedback";
import { toMessage } from "@/lib/errors";
import { canRevealInFolder, revealInFolder, saveDocument } from "@/lib/files";
import { formatClock } from "@/lib/format";
import { getTranscript } from "@/lib/ipc";
import type { TranscriptSegment } from "@/types/ipc";

/** 最終文字起こしの表示。会議中のリアルタイム結果ではなく、音声全体から作り直したもの。 */
export function TranscriptCard({
  meetingId,
  transcriptPath,
  reloadKey,
}: {
  meetingId: string;
  transcriptPath: string | null;
  reloadKey: number;
}) {
  const [segments, setSegments] = useState<TranscriptSegment[] | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const list = await getTranscript(meetingId);
        if (!cancelled) setSegments(list);
      } catch (e) {
        if (!cancelled) setError(toMessage(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [meetingId, reloadKey]);

  if (!segments || segments.length === 0) {
    return null;
  }

  const shown = expanded ? segments : segments.slice(0, 20);

  return (
    <Card
      title="文字起こし"
      actions={<span className="text-sm text-muted">{segments.length} 件</span>}
    >
      <div className="stack gap-12">
        {error && <div className="text-sm text-muted">{error}</div>}
        <div className="transcript">
          {shown.map((s) => (
            <div key={s.id} className="transcript__row">
              <span className="transcript__time">{formatClock(s.startMs)}</span>
              <span className="transcript__text">{s.text}</span>
            </div>
          ))}
          {!expanded && segments.length > shown.length && (
            <div className="transcript__row">
              <span className="transcript__time" />
              <span className="transcript__text text-muted">
                ほか {segments.length - shown.length} 件
              </span>
            </div>
          )}
        </div>
        <div className="row gap-12">
          {segments.length > 20 && (
            <Button onClick={() => setExpanded((v) => !v)}>
              {expanded ? "先頭だけ表示" : "すべて表示"}
            </Button>
          )}
          <Button
            icon="download"
            onClick={() => {
              void saveDocument(meetingId, "transcript", "transcript.txt").catch((e) =>
                setError(toMessage(e)),
              );
            }}
          >
            文字起こしを保存
          </Button>
          {canRevealInFolder && transcriptPath && (
            <Button
              icon="folder"
              onClick={() => {
                void revealInFolder(transcriptPath).catch((e) => setError(toMessage(e)));
              }}
            >
              保存場所を開く
            </Button>
          )}
        </div>
      </div>
    </Card>
  );
}

export function TranscriptEmpty() {
  return (
    <EmptyState title="文字起こしはまだありません">
      <p>会議終了後の処理が完了すると表示されます。</p>
    </EmptyState>
  );
}
