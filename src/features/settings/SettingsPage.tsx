import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { FieldRow, SwitchField } from "@/components/ui/Field";
import { Alert, Badge, ErrorBanner } from "@/components/ui/Feedback";
import { toMessage } from "@/lib/errors";
import {
  checkComponents,
  getSettings,
  getSystemInfo,
  listInputDevices,
  resetSettings,
  updateSettings,
} from "@/lib/ipc";
import type { AppSettings, AudioInputDevice, ComponentStatus, SystemInfo } from "@/types/ipc";

import { ModelSection } from "./ModelSection";

export function SettingsPage() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [components, setComponents] = useState<ComponentStatus[]>([]);
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [devices, setDevices] = useState<AudioInputDevice[]>([]);
  const [deviceError, setDeviceError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setError(null);
      const [s, c, i] = await Promise.all([getSettings(), checkComponents(), getSystemInfo()]);
      setSettings(s);
      setComponents(c);
      setInfo(i);
    } catch (e) {
      setError(toMessage(e));
    }

    // マイクの列挙は失敗しても他の設定は使えるようにする。
    try {
      setDeviceError(null);
      setDevices(await listInputDevices());
    } catch (e) {
      setDeviceError(toMessage(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  function patch(changes: Partial<AppSettings>) {
    setSettings((prev) => (prev ? { ...prev, ...changes } : prev));
    setSaved(false);
  }

  async function save() {
    if (!settings || busy) return;
    setBusy(true);
    setError(null);
    try {
      const next = await updateSettings(settings);
      setSettings(next);
      setComponents(await checkComponents());
      setInfo(await getSystemInfo());
      setSaved(true);
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function restoreDefaults() {
    setBusy(true);
    setError(null);
    try {
      setSettings(await resetSettings());
      setSaved(true);
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function chooseFolder() {
    if (!settings) return;
    try {
      const picked = await openDialog({
        directory: true,
        multiple: false,
        title: "会議の保存先フォルダを選択",
        defaultPath: settings.meetingsDir,
      });
      if (typeof picked === "string") {
        patch({ meetingsDir: picked });
      }
    } catch (e) {
      setError(toMessage(e));
    }
  }

  if (!settings) {
    return (
      <div className="page">
        <ErrorBanner message={error} />
        {!error && <div className="empty">読み込み中…</div>}
      </div>
    );
  }

  const missing = components.filter((c) => c.state === "missing");

  return (
    <div className="page">
      <div className="page__head">
        <div className="page__head-text">
          <h1>設定</h1>
          <p className="page__desc">
            音声認識、AI分析、保存先などを管理します。すべての処理はこのPC内で行われ、
            会議の音声や文字起こしが外部へ送信されることはありません。
          </p>
        </div>
      </div>

      <ErrorBanner message={error} />
      {saved && !error && (
        <div style={{ marginBottom: 16 }}>
          <Alert tone="ok" title="保存しました" />
        </div>
      )}

      <div className="stack gap-16">
        {missing.length > 0 && (
          <Alert tone="warn" title="必要なコンポーネントがありません">
            {missing.map((c) => c.name).join(" / ")} が未導入です。
            該当する機能は使用できませんが、録音は問題なく行えます。
          </Alert>
        )}

        <Card title="保存" icon="folder" iconTone="orange">
          <div className="stack gap-16">
            <FieldRow
              label="保存先フォルダ"
              hint="会議ごとにフォルダを作り、音声・議事録・まとめを保存します。"
            >
              {(id) => (
                <div className="row gap-8">
                  <input
                    id={id}
                    className="input"
                    value={settings.meetingsDir}
                    onChange={(e) => patch({ meetingsDir: e.target.value })}
                  />
                  <Button onClick={() => void chooseFolder()}>変更</Button>
                  <Button
                    icon="folder"
                    onClick={() => {
                      void revealItemInDir(settings.meetingsDir).catch((e) =>
                        setError(toMessage(e)),
                      );
                    }}
                  >
                    開く
                  </Button>
                </div>
              )}
            </FieldRow>

            <FieldRow
              label="必要な空き容量"
              hint="3時間の録音でおよそ 350MB を使用します。これを下回ると録音を開始しません。"
            >
              {(id) => (
                <div className="row gap-8">
                  <input
                    id={id}
                    className="input"
                    type="number"
                    style={{ maxWidth: 160 }}
                    value={settings.minFreeDiskMb}
                    onChange={(e) => patch({ minFreeDiskMb: Number(e.target.value) || 0 })}
                  />
                  <span className="text-sm text-muted">MB</span>
                </div>
              )}
            </FieldRow>
          </div>
        </Card>

        <ModelSection settings={settings} onPatch={patch} />

        <Card title="AI分析" icon="sparkles" iconTone="purple">
          <div className="stack gap-16">
            <FieldRow
              label="接続先"
              hint="ローカルで動作する Ollama のアドレスです。外部サービスは使用しません。"
            >
              {(id) => (
                <input
                  id={id}
                  className="input"
                  value={settings.llmEndpoint}
                  onChange={(e) => patch({ llmEndpoint: e.target.value })}
                />
              )}
            </FieldRow>
            <FieldRow label="モデル名" hint="例: qwen2.5:7b-instruct">
              {(id) => (
                <input
                  id={id}
                  className="input"
                  value={settings.llmModel}
                  onChange={(e) => patch({ llmModel: e.target.value })}
                />
              )}
            </FieldRow>

            <div className="stack gap-4">
              <SwitchField
                label="リアルタイム文字起こし"
                description="会議中に発言をその場で文字にします。オフにしても録音と最終文字起こしには影響しません。"
                checked={settings.realtimeTranscriptionEnabled}
                onChange={(v) => patch({ realtimeTranscriptionEnabled: v })}
              />
              <SwitchField
                label="リアルタイムAI分析"
                description="会議中に決定事項やネクストアクションを推定して表示します。"
                checked={settings.realtimeAnalysisEnabled}
                onChange={(v) => patch({ realtimeAnalysisEnabled: v })}
              />
              <SwitchField
                label="会議終了後にAIまとめを作成する"
                description="オフにすると音声と文字起こしのみを保存します。"
                checked={settings.aiSummaryEnabled}
                onChange={(v) => patch({ aiSummaryEnabled: v })}
              />
            </div>

            <FieldRow label="AI分析の間隔" hint="会議中に分析を実行する間隔です（20〜600秒）。">
              {(id) => (
                <div className="row gap-8">
                  <input
                    id={id}
                    className="input"
                    type="number"
                    style={{ maxWidth: 160 }}
                    value={settings.realtimeAnalysisIntervalSecs}
                    onChange={(e) =>
                      patch({ realtimeAnalysisIntervalSecs: Number(e.target.value) || 0 })
                    }
                  />
                  <span className="text-sm text-muted">秒</span>
                </div>
              )}
            </FieldRow>
          </div>
        </Card>

        <Card title="録音" icon="mic" iconTone="green">
          <div className="stack gap-16">
            {deviceError && (
              <Alert tone="warn" title="マイクを列挙できません">
                {deviceError}
              </Alert>
            )}
            <FieldRow
              label="入力デバイス"
              hint="「自動」にすると、OSで既定に設定されているマイクを使います。"
            >
              {(id) => (
                <select
                  id={id}
                  className="select"
                  value={settings.inputDevice ?? ""}
                  onChange={(e) => patch({ inputDevice: e.target.value || null })}
                >
                  <option value="">自動（OSの既定のマイク）</option>
                  {devices.map((d) => (
                    <option key={d.name} value={d.name}>
                      {d.name}
                      {d.isDefault ? "（既定）" : ""}
                    </option>
                  ))}
                </select>
              )}
            </FieldRow>
          </div>
        </Card>

        <Card
          title="動作状況"
          subtitle="問題がある場合はここを確認してください。"
          icon="help"
          iconTone="blue"
        >
          <ul className="divided-list">
            {components.map((c) => (
              <li key={c.name}>
                <div className="row row--top gap-12">
                  <Badge
                    tone={c.state === "ready" ? "ok" : c.state === "missing" ? "danger" : "neutral"}
                  >
                    {c.state === "ready" ? "利用可能" : c.state === "missing" ? "未導入" : "未確認"}
                  </Badge>
                  <div className="grow">
                    <div className="bold">{c.name}</div>
                    <div className="text-sm text-muted">{c.message}</div>
                    {c.setupHint && <div className="text-sm text-muted">{c.setupHint}</div>}
                  </div>
                </div>
              </li>
            ))}
          </ul>
        </Card>

        {info && (
          <Card title="このアプリについて" icon="document" iconTone="blue">
            <div className="stack gap-16">
              <dl className="detail-grid">
                <dt>バージョン</dt>
                <dd>{info.appVersion}</dd>
                <dt>OS</dt>
                <dd>{info.os}</dd>
                <dt>データフォルダ</dt>
                <dd className="mono">{info.appDataDir}</dd>
                <dt>モデル保存先</dt>
                <dd className="mono">{info.modelsDir}</dd>
                <dt>ログ</dt>
                <dd className="mono">{info.logDir}</dd>
              </dl>
              <div>
                <Button
                  icon="folder"
                  onClick={() => {
                    void revealItemInDir(info.logDir).catch((e) => setError(toMessage(e)));
                  }}
                >
                  ログを開く
                </Button>
              </div>
            </div>
          </Card>
        )}

        <div className="row gap-12" style={{ justifyContent: "flex-end" }}>
          <Button variant="ghost" onClick={() => void restoreDefaults()} disabled={busy}>
            既定値に戻す
          </Button>
          <Button variant="primary" large onClick={() => void save()} disabled={busy}>
            {busy ? "保存中…" : "保存"}
          </Button>
        </div>
      </div>
    </div>
  );
}
