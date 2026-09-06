/** 表示用の整形。ロジックは UI から分離してここに集約する。 */

/**
 * ミリ秒 → "1時間03分" 形式。一覧向け。
 *
 * 1 分未満は「0分」だと録音できたのか分からないため、秒で表示する。
 */
export function formatDurationJa(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "—";
  const totalSeconds = Math.floor(ms / 1000);
  if (totalSeconds < 60) return `${totalSeconds}秒`;

  const totalMinutes = Math.floor(totalSeconds / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours === 0) return `${minutes}分`;
  return `${hours}時間${String(minutes).padStart(2, "0")}分`;
}

/** ミリ秒 → "01:03:42" 形式。会議中の経過時間表示向け。 */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return [h, m, s].map((v) => String(v).padStart(2, "0")).join(":");
}

/** ISO8601 → "2026/09/05" */
export function formatDate(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return `${d.getFullYear()}/${String(d.getMonth() + 1).padStart(2, "0")}/${String(
    d.getDate(),
  ).padStart(2, "0")}`;
}

/** ISO8601 → "14:30" */
export function formatTime(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** ISO8601 → "2026/09/05 14:30" */
export function formatDateTime(iso: string | null): string {
  if (!iso) return "";
  const date = formatDate(iso);
  const time = formatTime(iso);
  return date ? `${date} ${time}` : "";
}

/** 改行区切りテキスト → 配列（空行は除去） */
export function linesToArray(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

export function arrayToLines(items: readonly string[]): string {
  return items.join("\n");
}
