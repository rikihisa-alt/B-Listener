/**
 * マイク入力レベルの表示。
 *
 * 目的は「音が拾えているか」をひと目で確認できるようにすること。
 * 装飾ではないので、動きは最小限にする。
 */
export function LevelMeter({ level, muted }: { level: number; muted: boolean }) {
  const clamped = Math.max(0, Math.min(1, level));
  // 振幅そのままだと通常の会話でほとんど動かないため、対数寄りに補正する。
  const display = muted ? 0 : Math.sqrt(clamped);

  return (
    <div className="level-meter" title={`入力レベル ${Math.round(clamped * 100)}%`}>
      <div className="level-meter__fill" style={{ width: `${display * 100}%` }} />
    </div>
  );
}
