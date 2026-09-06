import { useRef, useState } from "react";

import { Icon } from "./Icon";

interface TagInputProps {
  values: readonly string[];
  onChange: (values: string[]) => void;
  placeholder?: string;
  /** 入力欄の説明。スクリーンリーダー向けにも使う */
  label: string;
  id?: string;
}

/**
 * 参加者・議題・専門用語などを「タグ」として並べて入力する。
 *
 * Enter / 読点 / カンマ で確定する。日本語入力の確定 Enter を誤ってタグ化しないよう、
 * IME 変換中（composing）は無視する。
 */
export function TagInput({ values, onChange, placeholder, label, id }: TagInputProps) {
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  /**
   * IME 変換中かどうか。
   *
   * state ではなく ref で持つ。state だと `compositionend` の直後に
   * 押された Enter が「まだ変換中」と誤判定され、タグが追加できなくなる
   * （React の再描画が間に合わないため）。
   */
  const composingRef = useRef(false);

  function commit(raw: string) {
    // 「、」「,」区切りの貼り付けにも対応する
    const parts = raw
      .split(/[,、]/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0 && !values.includes(s));
    if (parts.length > 0) {
      onChange([...values, ...parts]);
    }
    setDraft("");
  }

  function remove(index: number) {
    onChange(values.filter((_, i) => i !== index));
  }

  return (
    <div
      className="tag-input"
      onClick={() => inputRef.current?.focus()}
      role="group"
      aria-label={label}
    >
      {values.map((value, index) => (
        <span className="tag" key={`${value}-${index}`}>
          <span className="tag__text">{value}</span>
          <button
            type="button"
            className="tag__remove"
            aria-label={`${value} を削除`}
            onClick={(e) => {
              e.stopPropagation();
              remove(index);
            }}
          >
            <Icon name="close" size={12} />
          </button>
        </span>
      ))}
      <input
        ref={inputRef}
        id={id}
        className="tag-input__entry"
        value={draft}
        placeholder={values.length === 0 ? placeholder : ""}
        onChange={(e) => setDraft(e.target.value)}
        onCompositionStart={() => {
          composingRef.current = true;
        }}
        onCompositionEnd={() => {
          composingRef.current = false;
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            // 変換確定の Enter ではタグ化せず、確定後の Enter で追加する。
            // ブラウザによって isComposing の立ち方が違うため両方を見る。
            if (composingRef.current || e.nativeEvent.isComposing) return;
            e.preventDefault();
            commit(draft);
          } else if (e.key === "Backspace" && draft === "" && values.length > 0) {
            remove(values.length - 1);
          } else if (e.key === "," || e.key === "、") {
            e.preventDefault();
            commit(draft);
          }
        }}
        onBlur={() => commit(draft)}
      />
    </div>
  );
}
