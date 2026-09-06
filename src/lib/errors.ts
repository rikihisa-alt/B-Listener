import type { AppErrorPayload } from "@/types/ipc";

/** invoke の reject 値が Rust の AppError かどうかを判定する。 */
export function isAppError(value: unknown): value is AppErrorPayload {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    typeof (value as { message: unknown }).message === "string"
  );
}

/**
 * 何が来ても画面に出せる日本語メッセージへ変換する。
 * エラーを握り潰さないため、コンソールにも必ず記録する。
 */
export function toMessage(error: unknown): string {
  if (isAppError(error)) {
    console.error(`[${error.code}] ${error.message}`, error);
    return error.message;
  }
  if (error instanceof Error) {
    console.error(error);
    return error.message;
  }
  console.error("不明なエラー", error);
  return "予期しないエラーが発生しました。";
}
