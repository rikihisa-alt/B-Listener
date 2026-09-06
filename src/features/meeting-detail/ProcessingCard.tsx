import { useEffect, useState } from "react";

import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { Alert, Badge } from "@/components/ui/Feedback";
import { onPipelineDone, onPipelineFailed, onPipelineProgress } from "@/lib/events";
import { toMessage } from "@/lib/errors";
import { getPipelineStatus, retryPipeline, runPipeline } from "@/lib/ipc";
import type { JobState, PipelineFailure, PipelineProgress, PipelineStatus } from "@/types/ipc";

const JOB_STATE_LABEL: Record<JobState, string> = {
  pending: "待機",
  running: "実行中",
  done: "完了",
  failed: "失敗",
};

function jobTone(state: JobState): "neutral" | "ok" | "warn" | "danger" {
  if (state === "done") return "ok";
  if (state === "running") return "warn";
  if (state === "failed") return "danger";
  return "neutral";
}

/**
 * 会議終了後の自動処理の状態を表示する。
 *
 * 失敗しても「何が残っているか」を必ず伝える。
 * 録音・文字起こしは AI 処理と独立しているため、部分的な成功に価値がある。
 */
export function ProcessingCard({
  meetingId,
  onChanged,
}: {
  meetingId: string;
  onChanged: () => void;
}) {
  const [status, setStatus] = useState<PipelineStatus | null>(null);
  const [progress, setProgress] = useState<PipelineProgress | null>(null);
  const [failure, setFailure] = useState<PipelineFailure | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /** 完了後は既定で折りたたむ。必要なときだけステップの内訳を見せる。 */
  const [expanded, setExpanded] = useState(false);

  async function refresh() {
    try {
      setStatus(await getPipelineStatus(meetingId));
    } catch (e) {
      setError(toMessage(e));
    }
  }

  useEffect(() => {
    void refresh();
    // meetingId が変わったら購読も張り直す
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meetingId]);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let disposed = false;
    const register = (p: Promise<() => void>) => {
      void p.then((un) => (disposed ? un() : unlisteners.push(un)));
    };

    register(
      onPipelineProgress((p) => {
        if (p.meetingId !== meetingId) return;
        setProgress(p);
        setFailure(null);
      }),
    );
    register(
      onPipelineDone((id) => {
        if (id !== meetingId) return;
        setProgress(null);
        void refresh();
        onChanged();
      }),
    );
    register(
      onPipelineFailed((f) => {
        if (f.meetingId !== meetingId) return;
        setProgress(null);
        setFailure(f);
        void refresh();
        onChanged();
      }),
    );

    return () => {
      disposed = true;
      unlisteners.forEach((un) => un());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meetingId]);

  async function start(fromTranscription: boolean) {
    setBusy(true);
    setError(null);
    setFailure(null);
    try {
      if (status?.jobs.length) {
        await retryPipeline(meetingId, fromTranscription);
      } else {
        await runPipeline(meetingId);
      }
      await refresh();
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setBusy(false);
    }
  }

  if (!status) return null;

  const running = status.running || status.status === "processing";
  const hasJobs = status.jobs.length > 0;
  const failedJob = status.jobs.find((j) => j.state === "failed");
  const settled = !running && !failure && !failedJob && status.status === "completed";

  // 何も起きていない会議（未開始など）にはカードを出さない。
  if (!running && !failure && !hasJobs && status.status !== "failed") {
    return null;
  }

  // 完了していて失敗もない場合は、1 行にたたんで画面をうるさくしない。
  if (settled && !expanded) {
    return (
      <Card title="会議終了後の処理" icon="check-circle" iconTone="green">
        <div className="row gap-16">
          <span className="grow text-muted">
            すべての処理が完了しています（{status.jobs.length} ステップ）。
          </span>
          <Button variant="link" onClick={() => setExpanded(true)}>
            内訳を見る
          </Button>
        </div>
      </Card>
    );
  }

  return (
    <Card
      title="会議終了後の処理"
      icon={failure || failedJob ? "alert" : running ? "refresh" : "check-circle"}
      iconTone={failure || failedJob ? "red" : running ? "blue" : "green"}
      actions={
        settled ? (
          <Button variant="link" onClick={() => setExpanded(false)}>
            たたむ
          </Button>
        ) : undefined
      }
    >
      <div className="stack gap-16">
        {error && <Alert tone="danger" title="エラー">{error}</Alert>}

        {running && (
          <div className="stack gap-8">
            <div className="text-sm">
              {progress
                ? `${progress.label}（${progress.stepIndex + 1} / ${progress.stepTotal}）`
                : "処理を準備しています…"}
            </div>
            <ProgressBar percent={progress?.percent ?? 0} label={progress?.label} />
            {progress?.message && <div className="text-sm text-muted">{progress.message}</div>}
            <div className="text-sm text-muted">
              この処理には時間がかかります。ウィンドウを閉じずにお待ちください。
            </div>
          </div>
        )}

        {failure && (
          <Alert tone="danger" title={`${failure.label}に失敗しました`}>
            <div>{failure.message}</div>
            {failure.preserved.length > 0 && (
              <div style={{ marginTop: 6 }}>
                <strong>保存されているもの:</strong> {failure.preserved.join(" / ")}
              </div>
            )}
          </Alert>
        )}

        {hasJobs && (
          <ul className="divided-list">
            {status.jobs.map((job) => (
              <li key={job.step}>
                <div className="row gap-12" style={{ alignItems: "flex-start" }}>
                  <Badge tone={jobTone(job.state)}>{JOB_STATE_LABEL[job.state]}</Badge>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontWeight: 600 }}>{job.label}</div>
                    {job.error && <div className="text-sm text-muted">{job.error}</div>}
                  </div>
                </div>
              </li>
            ))}
          </ul>
        )}

        {!running && (
          <div className="row gap-12">
            <Button variant="primary" onClick={() => void start(false)} disabled={busy}>
              {hasJobs ? "続きから再実行" : "処理を開始"}
            </Button>
            {hasJobs && (
              <Button onClick={() => void start(true)} disabled={busy}>
                文字起こしからやり直す
              </Button>
            )}
          </div>
        )}
      </div>
    </Card>
  );
}
