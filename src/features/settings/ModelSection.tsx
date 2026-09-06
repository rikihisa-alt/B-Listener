import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { FieldRow } from "@/components/ui/Field";
import { Alert, Badge } from "@/components/ui/Feedback";
import { ProgressBar } from "@/components/ui/ProgressBar";
import {
  onModelDownloadDone,
  onModelDownloadFailed,
  onModelDownloadProgress,
} from "@/lib/events";
import { toMessage } from "@/lib/errors";
import { cancelModelDownload, downloadWhisperModel, listWhisperModels } from "@/lib/ipc";
import type { AppSettings, ModelDownloadProgress, ModelStatus } from "@/types/ipc";

/**
 * 音声認識モデルの選択とダウンロード。
 *
 * 利用者に「Whisper」という技術を意識させないよう、
 * 選択肢は「速い／高精度」という観点で説明する。
 */
export function ModelSection({
  settings,
  onPatch,
}: {
  settings: AppSettings;
  onPatch: (changes: Partial<AppSettings>) => void;
}) {
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [progress, setProgress] = useState<ModelDownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setModels(await listWhisperModels());
    } catch (e) {
      setError(toMessage(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let disposed = false;
    const register = (p: Promise<() => void>) => {
      void p.then((un) => (disposed ? un() : unlisteners.push(un)));
    };

    register(onModelDownloadProgress((p) => setProgress(p)));
    register(
      onModelDownloadDone(() => {
        setProgress(null);
        setBusy(false);
        void refresh();
      }),
    );
    register(
      onModelDownloadFailed((message) => {
        setProgress(null);
        setBusy(false);
        setError(message);
        void refresh();
      }),
    );

    return () => {
      disposed = true;
      unlisteners.forEach((un) => un());
    };
  }, [refresh]);

  async function download(modelId: string) {
    setBusy(true);
    setError(null);
    setProgress({ modelId, downloadedBytes: 0, totalBytes: 0, percent: 0 });
    try {
      await downloadWhisperModel(modelId);
    } catch (e) {
      setError(toMessage(e));
      setProgress(null);
      setBusy(false);
    }
  }

  const installed = models.filter((m) => m.installed);
  const realtimeCandidates = models.filter((m) => m.suitableForRealtime);

  return (
    <Card title="音声認識" icon="waveform" iconTone="green">
      <div className="stack gap-16">
        {error && (
          <Alert tone="danger" title="エラー">
            {error}
          </Alert>
        )}

        {installed.length === 0 && (
          <Alert tone="warn" title="必要なコンポーネントがありません">
            音声認識モデルがまだインストールされていません。
            下の一覧からダウンロードすると、会議終了後の文字起こしが使えるようになります。
            <br />
            モデルが無い状態でも録音は正常に行えます。
          </Alert>
        )}

        <FieldRow
          label="最終文字起こし"
          hint="会議終了後に音声全体を処理します。精度を優先してください。"
        >
          {(id) => (
            <select
              id={id}
              className="select"
              value={settings.whisperModel}
              onChange={(e) => onPatch({ whisperModel: e.target.value })}
            >
              {models.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                  {m.installed ? "" : "（未ダウンロード）"}
                </option>
              ))}
            </select>
          )}
        </FieldRow>

        <FieldRow
          label="リアルタイム"
          hint="会議中に使用します。速度を優先してください。"
        >
          {(id) => (
            <select
              id={id}
              className="select"
              value={settings.whisperRealtimeModel}
              onChange={(e) => onPatch({ whisperRealtimeModel: e.target.value })}
            >
              {realtimeCandidates.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                  {m.installed ? "" : "（未ダウンロード）"}
                </option>
              ))}
            </select>
          )}
        </FieldRow>

        {progress && (
          <div className="stack gap-8">
            <div className="text-sm">
              {progress.modelId} をダウンロードしています
              {progress.totalBytes > 0 &&
                `（${formatMb(progress.downloadedBytes)} / ${formatMb(progress.totalBytes)}）`}
            </div>
            <ProgressBar percent={progress.percent} label="モデルのダウンロード" />
            <div>
              <Button
                onClick={() => {
                  void cancelModelDownload().catch((e) => setError(toMessage(e)));
                }}
              >
                ダウンロードを中止
              </Button>
            </div>
          </div>
        )}

        <div>
          <div className="field__label" style={{ marginBottom: 10 }}>
            モデルの管理
          </div>
          <div>
            {models.map((m) => (
              <div key={m.id} className="model-row">
                <Badge tone={m.installed ? "ok" : "neutral"}>
                  {m.installed ? "導入済み" : "未導入"}
                </Badge>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontWeight: 600 }}>{m.label}</div>
                  <div className="text-sm text-muted">
                    ダウンロードサイズ 約 {m.approxSizeMb} MB
                    {m.installed && ` ・ 実サイズ ${formatMb(m.sizeBytes)}`}
                  </div>
                  {m.installed && <div className="mono text-faint">{m.path}</div>}
                </div>
                {!m.installed && (
                  <Button onClick={() => void download(m.id)} disabled={busy}>
                    ダウンロード
                  </Button>
                )}
              </div>
            ))}
          </div>
        </div>
      </div>
    </Card>
  );
}

function formatMb(bytes: number): string {
  return `${Math.round(bytes / 1024 / 1024)} MB`;
}
