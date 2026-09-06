/** 進捗表示。長い処理でユーザーを不安にさせないための最小限の表示。 */
export function ProgressBar({ percent, label }: { percent: number; label?: string }) {
  const clamped = Math.max(0, Math.min(100, Math.round(percent)));
  return (
    <div className="progress">
      <div
        className="progress__track"
        role="progressbar"
        aria-valuenow={clamped}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={label ?? "進捗"}
      >
        <div className="progress__fill" style={{ width: `${clamped}%` }} />
      </div>
      <span className="progress__value mono">{clamped}%</span>
    </div>
  );
}
