import { useState } from "react";
import { useNavigate } from "react-router-dom";

import { Button } from "@/components/ui/Button";
import { FieldRow } from "@/components/ui/Field";
import { Alert, ErrorBanner } from "@/components/ui/Feedback";
import { Icon } from "@/components/ui/Icon";
import { TagInput } from "@/components/ui/TagInput";
import { toMessage } from "@/lib/errors";
import { createMeeting, saveMeetingContext } from "@/lib/ipc";
import type { ContextTermInput, MeetingContextInput, TermCategory } from "@/types/ipc";

import "./new-meeting.css";

/**
 * 追加で分類できる固有名詞のカテゴリ。
 *
 * 仕様では人名・利用者名・顧客名などを個別に受け取るが、
 * 毎回すべてを埋めさせると入力の負担が大きい。
 * 既定では 1 つの入力欄にまとめ、必要な人だけ分類を開けるようにする。
 */
const DETAIL_CATEGORIES: ReadonlyArray<{ key: TermCategory; label: string; placeholder: string }> = [
  { key: "person", label: "人名", placeholder: "例: 吉田、生田" },
  { key: "user", label: "利用者名", placeholder: "例: A様、B様" },
  { key: "customer", label: "顧客名", placeholder: "例: ○○様" },
  { key: "company", label: "会社名", placeholder: "例: ○○株式会社" },
  { key: "service", label: "サービス名", placeholder: "例: 訪問介護" },
  { key: "abbrev", label: "略称", placeholder: "例: A勤、C勤" },
];

type DetailTerms = Partial<Record<TermCategory, string[]>>;

/**
 * 新しい会議の作成と事前情報の入力。
 *
 * 事前情報は**すべて任意**。何も入力しなくても会議は正常に記録される。
 * 入力した内容は文字起こしの補正と AI 分析のコンテキストに使われる。
 */
export function NewMeetingPage() {
  const navigate = useNavigate();

  // 基本情報
  const [title, setTitle] = useState("");
  const [scheduledAt, setScheduledAt] = useState("");
  const [participants, setParticipants] = useState<string[]>([]);

  // 議事情報
  const [agendas, setAgendas] = useState<string[]>([]);
  const [goal, setGoal] = useState("");
  const [carryover, setCarryover] = useState("");
  const [notes, setNotes] = useState("");

  // AI補助情報
  const [terms, setTerms] = useState<string[]>([]);
  const [detailOpen, setDetailOpen] = useState(false);
  const [detailTerms, setDetailTerms] = useState<DetailTerms>({});

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function buildTermInputs(): ContextTermInput[] {
    const out: ContextTermInput[] = terms.map((term) => ({
      term,
      category: "jargon",
      reading: "",
    }));
    for (const { key } of DETAIL_CATEGORIES) {
      for (const term of detailTerms[key] ?? []) {
        out.push({ term, category: key, reading: "" });
      }
    }
    return out;
  }

  /**
   * 会議を作成して録音画面へ進む。
   *
   * 「スキップ」と「この内容で開始」は同じ動作にしている。
   * 入力済みの内容を捨てると利用者が驚くため、常に入力内容を保存する。
   * ボタンを 2 つ置いているのは「埋めなくても始められる」ことを伝えるためのもの。
   */
  async function start() {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const meeting = await createMeeting(title.trim() || undefined);

      const context: MeetingContextInput = {
        scheduledAt: scheduledAt || undefined,
        goal: goal.trim() || undefined,
        carryover: carryover.trim() || undefined,
        notes: notes.trim() || undefined,
        participants,
        agendas,
        terms: buildTermInputs(),
      };
      const hasContext =
        Boolean(context.scheduledAt || context.goal || context.carryover || context.notes) ||
        participants.length > 0 ||
        agendas.length > 0 ||
        (context.terms?.length ?? 0) > 0;

      if (hasContext) {
        // 事前情報の保存に失敗しても会議自体は始められるようにする。
        try {
          await saveMeetingContext(meeting.id, context);
        } catch (e) {
          console.error("事前情報の保存に失敗しました", e);
        }
      }

      navigate(`/meetings/${meeting.id}/live`, { replace: true });
    } catch (e) {
      setError(toMessage(e));
      setBusy(false);
    }
  }

  return (
    <div className="page">
      <div className="page__head">
        <div className="page__head-text">
          <h1>新しい会議</h1>
          <p className="page__desc">
            事前情報を入力して、議事録の精度を高めます。すべて任意入力です。
          </p>
        </div>
      </div>

      <ErrorBanner message={error} />

      <div className="stack gap-16">
        <FormSection number={1} title="基本情報">
          <FieldRow label="会議名" hint="未入力の場合は日時から自動で付けます。">
            {(id) => (
              <input
                id={id}
                className="input"
                value={title}
                placeholder="例: 9月 運営会議"
                onChange={(e) => setTitle(e.target.value)}
              />
            )}
          </FieldRow>

          <FieldRow label="会議日時">
            {(id) => (
              <input
                id={id}
                className="input"
                type="datetime-local"
                value={scheduledAt}
                onChange={(e) => setScheduledAt(e.target.value)}
              />
            )}
          </FieldRow>

          <FieldRow
            label="参加者"
            hint="入力して Enter で追加します。文字起こしの人名補正にも使われます。"
          >
            {(id) => (
              <TagInput
                id={id}
                label="参加者"
                values={participants}
                onChange={setParticipants}
                placeholder="例: 吉田、生田、ウィン"
              />
            )}
          </FieldRow>
        </FormSection>

        <FormSection number={2} title="議事情報">
          <FieldRow label="主な議題">
            {(id) => (
              <TagInput
                id={id}
                label="主な議題"
                values={agendas}
                onChange={setAgendas}
                placeholder="例: シフト調整、新規利用者対応"
              />
            )}
          </FieldRow>

          <FieldRow label="今回決めたいこと">
            {(id) => (
              <input
                id={id}
                className="input"
                value={goal}
                placeholder="例: 10月の人員配置、新規利用者A様の開始日"
                onChange={(e) => setGoal(e.target.value)}
              />
            )}
          </FieldRow>

          <FieldRow label="前回からの持ち越し事項">
            {(id) => (
              <input
                id={id}
                className="input"
                value={carryover}
                placeholder="例: A様の家族確認、10月シフト調整"
                onChange={(e) => setCarryover(e.target.value)}
              />
            )}
          </FieldRow>

          <FieldRow label="補足情報">
            {(id) => (
              <textarea
                id={id}
                className="textarea"
                rows={3}
                value={notes}
                placeholder={
                  "その他、共有しておきたい情報があれば入力してください\n（例：関連資料の有無、特に注目してほしい点 など）"
                }
                onChange={(e) => setNotes(e.target.value)}
              />
            )}
          </FieldRow>
        </FormSection>

        <FormSection number={3} title="AI補助情報">
          <FieldRow
            label="専門用語 / 固有名詞"
            hint="会議で出てくる言葉を入れておくと、文字起こしの誤変換が減ります。"
          >
            {(id) => (
              <TagInput
                id={id}
                label="専門用語・固有名詞"
                values={terms}
                onChange={setTerms}
                placeholder="例: A勤、C勤、国保連、訪問介護、訪問看護"
              />
            )}
          </FieldRow>

          <div>
            <Button
              variant="link"
              icon={detailOpen ? "chevron-down" : "arrow-right"}
              onClick={() => setDetailOpen((v) => !v)}
              aria-expanded={detailOpen}
            >
              {detailOpen ? "分類を閉じる" : "人名・会社名などを分類して入力する"}
            </Button>
          </div>

          {detailOpen &&
            DETAIL_CATEGORIES.map(({ key, label, placeholder }) => (
              <FieldRow key={key} label={label}>
                {(id) => (
                  <TagInput
                    id={id}
                    label={label}
                    values={detailTerms[key] ?? []}
                    onChange={(values) => setDetailTerms((prev) => ({ ...prev, [key]: values }))}
                    placeholder={placeholder}
                  />
                )}
              </FieldRow>
            ))}
        </FormSection>

        <Alert>
          <span>
            事前情報は補助として使われます。会議で実際に発言された内容が常に優先され、
            ここに書いただけの事柄が「決定事項」として議事録に入ることはありません。
          </span>
        </Alert>

        <div className="new-meeting__actions">
          <Button large onClick={() => void start()} disabled={busy}>
            スキップして会議開始
          </Button>
          <Button variant="primary" large icon="mic" onClick={() => void start()} disabled={busy}>
            {busy ? "準備中…" : "この内容で開始"}
          </Button>
        </div>
        <p className="text-sm text-muted" style={{ textAlign: "right" }}>
          <Icon name="help" size={13} /> どちらを押しても、入力済みの内容は保存されます。
        </p>
      </div>
    </div>
  );
}

function FormSection({
  number,
  title,
  children,
}: {
  number: number;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="card">
      <header className="card__head">
        <span className="section-number">{number}</span>
        <h2>{title}</h2>
      </header>
      <div className="card__body stack gap-16">{children}</div>
    </section>
  );
}
