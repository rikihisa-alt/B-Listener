import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";

import { Button } from "@/components/ui/Button";
import { Alert, ErrorBanner } from "@/components/ui/Feedback";
import { IconBadge } from "@/components/ui/Icon";
import { onDiskWarning, onRecordingError, onRecordingTick } from "@/lib/events";
import { toMessage } from "@/lib/errors";
import { formatClock, formatDate } from "@/lib/format";
import {
  getMeetingDetail,
  getRecordingState,
  pauseRecording,
  resumeRecording,
  startRecording,
  stopRecording,
} from "@/lib/ipc";
import type { MeetingDetail, RecordingSnapshot } from "@/types/ipc";

import { AnalysisSection } from "./AnalysisPanel";
import { LevelMeter } from "./LevelMeter";
import "./live.css";

/** この時間だけ入力レベルがゼロなら、マイクが拾えていない可能性を警告する。 */
const SILENCE_WARNING_MS = 5000;

export function LiveMeetingPage() {
  const { meetingId } = useParams<{ meetingId: string }>();
  const navigate = useNavigate();

  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [snapshot, setSnapshot] = useState<RecordingSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [deviceError, setDeviceError] = useState<string | null>(null);
  const [diskWarning, setDiskWarning] = useState<string | null>(null);
  const [confirmingStop, setConfirmingStop] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [starting, setStarting] = useState(true);

  /** 一度でも音を拾えたか。警告の誤検知を防ぐために保持する。 */
  const heardSound = useRef(false);
  if (snapshot && snapshot.level > 0.01) {
    heardSound.current = true;
  }

  // 会議情報の取得と、録音の開始（既に録音中ならそれを引き継ぐ）
  useEffect(() => {
    if (!meetingId) return;
    let cancelled = false;

    (async () => {
      try {
        const loaded = await getMeetingDetail(meetingId);
        if (cancelled) return;
        setDetail(loaded);

        const current = await getRecordingState();
        if (cancelled) return;

        if (current && current.meetingId === meetingId) {
          // 画面を再表示した場合など、進行中の録音をそのまま引き継ぐ。
          setSnapshot(current);
        } else if (current) {
          setError("別の会議を録音中です。先にそちらを終了してください。");
        } else {
          setSnapshot(await startRecording(meetingId));
        }
      } catch (e) {
        if (!cancelled) setError(toMessage(e));
      } finally {
        if (!cancelled) setStarting(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [meetingId]);

  // 経過時間・レベル・エラーの購読
  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let disposed = false;
    const register = (p: Promise<() => void>) => {
      void p.then((un) => (disposed ? un() : unlisteners.push(un)));
    };

    register(onRecordingTick(setSnapshot));
    register(onRecordingError(setDeviceError));
    register(onDiskWarning(setDiskWarning));

    return () => {
      disposed = true;
      unlisteners.forEach((un) => un());
    };
  }, []);

  const togglePause = useCallback(async () => {
    if (!snapshot) return;
    try {
      setError(null);
      setSnapshot(snapshot.state === "paused" ? await resumeRecording() : await pauseRecording());
    } catch (e) {
      setError(toMessage(e));
    }
  }, [snapshot]);

  async function finish() {
    if (!meetingId || stopping) return;
    setStopping(true);
    setError(null);
    try {
      await stopRecording();
      navigate(`/meetings/${meetingId}`, { replace: true });
    } catch (e) {
      // 停止に失敗しても音声は保存されている。その事実を明示して詳細画面へ誘導する。
      setError(
        `${toMessage(e)}\nここまでの録音は保存されています。会議の詳細画面から確認してください。`,
      );
      setStopping(false);
      setConfirmingStop(false);
    }
  }

  const paused = snapshot?.state === "paused";
  const elapsedMs = snapshot?.elapsedMs ?? 0;
  const showSilenceWarning =
    !!snapshot && !paused && elapsedMs > SILENCE_WARNING_MS && !heardSound.current;
  const meeting = detail?.meeting;

  return (
    <div className="live">
      <header className="live__head">
        <div className="grow">
          <div className="live__title truncate">{meeting?.title ?? "会議"}</div>
          <div className="live__sub">
            <span>{formatDate(meeting?.startedAt ?? meeting?.createdAt ?? null)}</span>
            {(detail?.participants.length ?? 0) > 0 && (
              <>
                <span className="live__sub-sep">|</span>
                <span>参加者 {detail?.participants.length}名</span>
              </>
            )}
            <span className="live__sub-sep">|</span>
            <span className="truncate">{snapshot?.deviceName ?? "マイク準備中"}</span>
          </div>
        </div>

        <div className="live__status">
          <span className={paused ? "live__rec live__rec--paused" : "live__rec"}>
            <span className="live__dot" aria-hidden="true" />
            {paused ? "一時停止中" : "録音中"}
          </span>
          <span className="live__clock">{formatClock(elapsedMs)}</span>
        </div>
      </header>

      <div className="live__alerts">
        <ErrorBanner message={error} />
        {deviceError && (
          <Alert tone="danger" title="録音デバイスのエラー">
            {deviceError}
          </Alert>
        )}
        {diskWarning && (
          <Alert tone="warn" title="空き容量が不足しています">
            {diskWarning}
          </Alert>
        )}
        {showSilenceWarning && (
          <Alert tone="warn" title="音声を検出できていません">
            マイクが音を拾えていない可能性があります。マイクの接続と、OSのプライバシー設定で
            このアプリにマイクの使用が許可されているかを確認してください。録音自体は継続しています。
          </Alert>
        )}
        {starting && !snapshot && !error && <Alert title="録音を開始しています…" />}
      </div>

      <div className="live__body">
        <section className="live__pane">
          <header className="live__pane-head">
            <IconBadge name="document" tone="blue" size={17} />
            <span className="live__pane-title grow">リアルタイム文字起こし</span>
          </header>
          <div className="live__pane-body">
            <div className="live__placeholder">
              リアルタイム文字起こしは Phase 7 で有効になります。
              <br />
              録音は正常に行われており、会議終了後に音声全体から高精度な文字起こしを作成します。
            </div>
          </div>
        </section>

        <section className="live__pane">
          <header className="live__pane-head">
            <IconBadge name="sparkles" tone="purple" size={17} />
            <span className="live__pane-title grow">AI分析</span>
            <span className="text-xs text-faint">未実行</span>
          </header>
          <div className="live__pane-body">
            <AnalysisSection
              title="現在の議題"
              icon="target"
              tone="blue"
              items={[]}
              emptyText="会議が進むと表示されます"
            />
            <AnalysisSection
              title="決定事項"
              icon="check-circle"
              tone="green"
              items={[]}
              emptyText="まだ決定事項はありません"
            />
            <AnalysisSection
              title="ネクストアクション"
              icon="list"
              tone="blue"
              items={[]}
              emptyText="まだネクストアクションはありません"
            />
            <AnalysisSection
              title="注意事項"
              icon="alert"
              tone="orange"
              items={[]}
              emptyText="特にありません"
            />
            <AnalysisSection
              title="未決事項"
              icon="help"
              tone="purple"
              items={[]}
              emptyText="特にありません"
            />
          </div>
        </section>
      </div>

      <footer className="live__foot">
        <div className="live__meter">
          <LevelMeter level={snapshot?.level ?? 0} muted={paused} />
          <span className="text-sm text-muted">
            {snapshot ? "マイク入力" : "—"}
          </span>
        </div>
        <div className="row gap-12">
          <Button
            large
            icon={paused ? "play" : "pause"}
            iconFilled={paused}
            onClick={() => void togglePause()}
            disabled={!snapshot || stopping}
          >
            {paused ? "録音を再開" : "一時停止"}
          </Button>
          <Button
            variant="danger"
            large
            icon="stop"
            onClick={() => setConfirmingStop(true)}
            disabled={!snapshot || stopping}
          >
            会議終了
          </Button>
        </div>
      </footer>

      {confirmingStop && (
        <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label="会議終了の確認">
          <div className="modal">
            <div className="row gap-12">
              <IconBadge name="alert" tone="red" />
              <h2 className="grow">会議を終了しますか？</h2>
            </div>
            <p className="text-muted">
              録音を停止し、音声ファイルを確定します。
              <br />
              その後、自動で文字起こしとAI分析を行います。
            </p>
            <div className="detail-grid">
              <dt>録音時間</dt>
              <dd className="bold tabular">{formatClock(elapsedMs)}</dd>
            </div>
            <div className="row gap-12" style={{ justifyContent: "flex-end" }}>
              <Button variant="ghost" onClick={() => setConfirmingStop(false)} disabled={stopping}>
                会議に戻る
              </Button>
              <Button variant="danger" icon="stop" onClick={() => void finish()} disabled={stopping}>
                {stopping ? "終了処理中…" : "会議を終了"}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
