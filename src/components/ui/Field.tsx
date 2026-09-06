import { useId } from "react";
import type { ReactNode } from "react";

interface FieldProps {
  label: string;
  /** 事前入力はすべて任意項目。任意であることを明示する */
  optional?: boolean;
  hint?: string;
  children: (id: string) => ReactNode;
}

export function Field({ label, optional = false, hint, children }: FieldProps) {
  const id = useId();
  return (
    <div className="field">
      <label className="field__label" htmlFor={id}>
        {label}
        {optional && <span className="field__optional">任意</span>}
      </label>
      {children(id)}
      {hint && <span className="field__hint">{hint}</span>}
    </div>
  );
}

/** ラベルを左、入力を右に置く行（事前入力画面のレイアウト） */
export function FieldRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: (id: string) => ReactNode;
}) {
  const id = useId();
  return (
    <div className="field-row">
      <label className="field__label" htmlFor={id}>
        {label}
      </label>
      <div className="stack gap-4">
        {children(id)}
        {hint && <span className="field__hint">{hint}</span>}
      </div>
    </div>
  );
}

interface TextFieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  optional?: boolean;
  hint?: string;
  placeholder?: string;
  type?: "text" | "datetime-local" | "number";
  disabled?: boolean;
}

export function TextField({
  label,
  value,
  onChange,
  optional,
  hint,
  placeholder,
  type = "text",
  disabled,
}: TextFieldProps) {
  return (
    <Field label={label} optional={optional} hint={hint}>
      {(id) => (
        <input
          id={id}
          className="input"
          type={type}
          value={value}
          placeholder={placeholder}
          disabled={disabled}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </Field>
  );
}

interface TextAreaFieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  optional?: boolean;
  hint?: string;
  placeholder?: string;
  rows?: number;
}

export function TextAreaField({
  label,
  value,
  onChange,
  optional,
  hint,
  placeholder,
  rows = 3,
}: TextAreaFieldProps) {
  return (
    <Field label={label} optional={optional} hint={hint}>
      {(id) => (
        <textarea
          id={id}
          className="textarea"
          rows={rows}
          value={value}
          placeholder={placeholder}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </Field>
  );
}

interface SwitchFieldProps {
  label: string;
  description?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
}

export function SwitchField({ label, description, checked, onChange, disabled }: SwitchFieldProps) {
  return (
    <label className="switch">
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span>
        <span style={{ fontWeight: 600 }}>{label}</span>
        {description && (
          <span className="field__hint" style={{ display: "block" }}>
            {description}
          </span>
        )}
      </span>
    </label>
  );
}
