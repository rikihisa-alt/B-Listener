import { useCallback, useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Alert, Badge, ErrorBanner } from "@/components/ui/Feedback";
import { Icon, IconBadge } from "@/components/ui/Icon";
import { STATUS_LABEL, statusTone } from "@/features/home/meetingStatus";
import { toMessage } from "@/lib/errors";
import {
  audioUrl,
  canRevealInFolder,
  revealInFolder,
  saveAudio,
  saveDocument,
} from "@/lib/files";
import { formatClock, formatDate, formatDurationJa } from "@/lib/format";
import {
  deleteMeeting,
  getMeetingDetail,
  listMeetingFiles,
  readMeetingDocument,
} from "@/lib/ipc";
import type { MeetingDetail, MeetingDocument, MeetingFile } from "@/types/ipc";

import { ProcessingCard } from "./ProcessingCard";
import { TextViewerModal } from "./TextViewerModal";
import { TranscriptCard } from "./TranscriptCard";
import "./detail.css";

interface ViewerState {
  title: string;
  icon: "document" | "sparkles";
  tone: "blue" | "purple";
  content: string;
}

export function MeetingDetailPage() {
  const { meetingId } = useParams<{ meetingId: string }>();
  const navigate = useNavigate();

  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [files, setFiles] = useState<MeetingFile[]>([]);
  const [minutes, setMinutes] = useState<string | null>(null);
  const [summary, setSummary] = useState<string | null>(null);
  const [viewer, setViewer] = useState<ViewerState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  /** 処理完了時に文字起こし表示を読み直すためのキー */
  const [reloadKey, setReloadKey] = useState(0);

  const load = useCallback(async () => {
    if (!meetingId) return;
    try {
      setError(null);
      const [loaded, fileList, minutesText, summaryText] = await Promise.all([
        getMeetingDetail(meetingId),
        listMeetingFiles(meetingId),
        readMeetingDocument(meetingId, "minutes"),
        readMeetingDocument(meetingId, "summary"),
      ]);
      setDetail(loaded);
      setFiles(fileList);
      setMinutes(minutesText);
      setSummary(summaryText);
      setReloadKey((k) => k + 1);
    } catch (e) {
      setError(toMessage(e));
    }
  }, [meetingId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function remove() {
    if (!meetingId) return;
    try {
      await deleteMeeting(meetingId);
      navigate("/meetings", { replace: true });
    } catch (e) {
      setError(toMessage(e));
      setConfirmingDelete(false);
    }
  }

  /** 成果物を保存する。会議フォルダの原本はそのまま残る。 */
  async function exportDocument(document: MeetingDocument, defaultName: string) {
    if (!meetingId) return;
    try {
      setError(null);
      await saveDocument(meetingId, document, defaultName);
    } catch (e) {
      setError(toMessage(e));
    }
  }

  /** 録音音声を保存する。 */
  async function exportAudio() {
    if (!meetingId) return;
    try {
      setError(null);
      await saveAudio(meetingId);
    } catch (e) {
      setError(toMessage(e));
    }
  }

  if (error && !detail) {
    return (
      <div className="page">
        <ErrorBanner message={error} />
        <div style={{ marginTop: 16 }}>
          <Button onClick={() => navigate("/meetings")}>会議一覧へ戻る</Button>
        </div>
      </div>
    );
  }

  if (!detail) {
    return <div className="page">読み込み中…</div>;
  }

  const { meeting } = detail;
  const inProgress = meeting.status === "recording" || meeting.status === "paused";

  return (
    <div className="page">
      <Link to="/meetings" className="detail-back">
        <Icon name="arrow-left" size={15} />
        会議一覧に戻る
      </Link>

      <div className="page__head">
        <div className="page__head-text">
          <div className="detail-title-row">
            <h1>{meeting.title}</h1>
            <Badge tone={statusTone(meeting.status)}>{STATUS_LABEL[meeting.status]}</Badge>
          </div>
          <div className="detail-meta">
            <span className="detail-meta__item">
              <Icon name="calendar" size={15} />
              {formatDate(meeting.startedAt ?? meeting.createdAt)}
            </span>
            {meeting.durationMs > 0 && (
              <span className="detail-meta__item">
                <Icon name="clock" size={15} />
                {formatDurationJa(meeting.durationMs)}
              </span>
            )}
            {detail.participants.length > 0 && (
              <span className="detail-meta__item">
                <Icon name="users" size={15} />
                {detail.participants.map((p) => p.name).join("、")}
              </span>
            )}
          </div>
        </div>
      </div>

      <ErrorBanner message={error} />

      <div className="stack gap-16">
        {meeting.status === "draft" && (
          <Card title="会議を開始する" icon="mic" iconTone="blue">
            <div className="stack gap-16">
              <p className="text-muted">
                「会議開始」を押すと録音がすぐに始まります。会議中は経過時間と入力レベルを確認できます。
              </p>
              <div>
                <Button
                  variant="primary"
                  large
                  icon="mic"
                  onClick={() => navigate(`/meetings/${meeting.id}/live`)}
                >
                  会議開始
                </Button>
              </div>
            </div>
          </Card>
        )}

        {inProgress && (
          <Card title="録音中の会議です" icon="alert" iconTone="orange">
            <div className="stack gap-16">
              <Alert tone="warn">
                会議中の画面に戻って終了操作を行ってください。
                アプリが前回異常終了した場合は、ホーム画面の復旧案内から音声を保存できます。
              </Alert>
              <div>
                <Button
                  variant="primary"
                  large
                  onClick={() => navigate(`/meetings/${meeting.id}/live`)}
                >
                  会議中の画面へ戻る
                </Button>
              </div>
            </div>
          </Card>
        )}

        {meeting.audioPath && (
          <div className="result-grid">
            {/* 音声 */}
            <div className="result-card">
              <div className="result-card__head">
                <IconBadge name="waveform" tone="green" />
                <div>
                  <div className="result-card__title">音声</div>
                  <div className="result-card__sub">会議の録音データ</div>
                </div>
              </div>
              <div className="result-card__body">
                <div className="result-card__duration">{formatClock(meeting.durationMs)}</div>
                <audio
                  controls
                  preload="none"
                  src={audioUrl(meeting.id, meeting.audioPath)}
                  style={{ width: "100%" }}
                >
                  お使いの環境では音声を再生できません。
                </audio>
              </div>
              <div className="result-card__actions">
                <Button variant="primary" icon="download" onClick={() => void exportAudio()}>
                  保存
                </Button>
                {canRevealInFolder && (
                  <Button
                    icon="folder"
                    onClick={() => {
                      void revealInFolder(meeting.audioPath ?? "").catch((e) =>
                        setError(toMessage(e)),
                      );
                    }}
                  >
                    保存場所を開く
                  </Button>
                )}
              </div>
            </div>

            {/* 議事録 */}
            <ResultDocumentCard
              title="議事録"
              subtitle="AIが作成した議事録"
              icon="document"
              tone="blue"
              content={minutes}
              pendingText="議事録の作成は Phase 5 で有効になります。文字起こしは下に表示されています。"
              onView={() =>
                minutes &&
                setViewer({ title: "議事録", icon: "document", tone: "blue", content: minutes })
              }
              onSave={() => void exportDocument("minutes", "minutes.md")}
            />

            {/* AIまとめ */}
            <ResultDocumentCard
              title="AIまとめ"
              subtitle="重要なポイントを要約"
              icon="sparkles"
              tone="purple"
              content={summary}
              pendingText="AIまとめの作成は Phase 4 で有効になります。"
              onView={() =>
                summary &&
                setViewer({ title: "AIまとめ", icon: "sparkles", tone: "purple", content: summary })
              }
              onSave={() => void exportDocument("summary", "summary.md")}
            />
          </div>
        )}

        {meetingId && <ProcessingCard meetingId={meetingId} onChanged={() => void load()} />}

        {meetingId && (
          <TranscriptCard
            meetingId={meetingId}
            transcriptPath={meeting.transcriptPath}
            reloadKey={reloadKey}
          />
        )}

        {files.length > 0 && (
          <Card
            title="出力ファイル"
            subtitle="このPC内に保存されています。外部へは送信されません。"
            icon="folder"
            iconTone="orange"
            actions={
              canRevealInFolder ? (
                <Button
                  icon="folder"
                  onClick={() => {
                    void revealInFolder(files[0]?.path ?? "").catch((e) => setError(toMessage(e)));
                  }}
                >
                  保存フォルダを開く
                </Button>
              ) : undefined
            }
          >
            <div className="file-row">
              {files.map((f) => (
                <div className="file-chip" key={f.path}>
                  <IconBadge name={f.isAudio ? "waveform" : "document"} tone={f.isAudio ? "green" : "blue"} size={16} />
                  <div>
                    <div className="file-chip__name">{f.name}</div>
                    <div className="file-chip__size">{formatBytes(f.sizeBytes)}</div>
                  </div>
                </div>
              ))}
            </div>
          </Card>
        )}

        <Card title="この会議を削除" icon="trash" iconTone="red">
          <div className="stack gap-12">
            <p className="text-sm text-muted">
              一覧から会議を削除します。録音ファイルを含む会議フォルダは削除されません。
              フォルダの削除は Finder / エクスプローラから行ってください。
            </p>
            {confirmingDelete ? (
              <div className="row gap-12">
                <Button variant="danger" icon="trash" onClick={() => void remove()}>
                  削除する
                </Button>
                <Button variant="ghost" onClick={() => setConfirmingDelete(false)}>
                  やめる
                </Button>
              </div>
            ) : (
              <div>
                <Button onClick={() => setConfirmingDelete(true)}>削除</Button>
              </div>
            )}
          </div>
        </Card>
      </div>

      {viewer && (
        <TextViewerModal
          title={viewer.title}
          icon={viewer.icon}
          iconTone={viewer.tone}
          content={viewer.content}
          onClose={() => setViewer(null)}
        />
      )}
    </div>
  );
}

function ResultDocumentCard({
  title,
  subtitle,
  icon,
  tone,
  content,
  pendingText,
  onView,
  onSave,
}: {
  title: string;
  subtitle: string;
  icon: "document" | "sparkles";
  tone: "blue" | "purple";
  content: string | null;
  pendingText: string;
  onView: () => void;
  onSave: () => void;
}) {
  const available = content !== null && content.trim().length > 0;
  return (
    <div className="result-card">
      <div className="result-card__head">
        <IconBadge name={icon} tone={tone} />
        <div>
          <div className="result-card__title">{title}</div>
          <div className="result-card__sub">{subtitle}</div>
        </div>
      </div>
      <div className="result-card__body">
        <div className="result-card__preview">
          {available ? `${content.slice(0, 260)}${content.length > 260 ? "\n…" : ""}` : pendingText}
        </div>
      </div>
      <div className="result-card__actions">
        <Button variant="primary" icon="document" onClick={onView} disabled={!available}>
          内容を見る
        </Button>
        <Button icon="download" onClick={onSave} disabled={!available}>
          保存
        </Button>
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
