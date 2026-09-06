/**
 * 実行environment（デスクトップ版 / ブラウザ版）の判定と、
 * 環境ごとに異なる操作の吸収。
 *
 * UI コンポーネントは `isDesktop` を直接見ず、
 * このモジュールが提供する関数を使う。
 */

/** Tauri（デスクトップ版）で動いているか。 */
export const isDesktop: boolean =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** ブラウザ版（社内サーバ経由）で動いているか。 */
export const isWeb = !isDesktop;

/**
 * ブラウザがマイクを使える状態か。
 *
 * ブラウザは「安全なコンテキスト」でしかマイクを許可しない。
 * `localhost` は安全とみなされるが、`http://192.168.x.x` のような
 * 平文の LAN アクセスでは拒否される。
 */
export function canUseBrowserMicrophone(): boolean {
  if (isDesktop) return true;
  if (typeof window === "undefined") return false;
  return window.isSecureContext && !!navigator.mediaDevices?.getUserMedia;
}

/** マイクが使えない理由を、利用者が対処できる言葉で返す。 */
export function microphoneBlockedReason(): string | null {
  if (canUseBrowserMicrophone()) return null;
  if (typeof window === "undefined") return "この環境ではマイクを使用できません。";

  if (!window.isSecureContext) {
    return (
      "このブラウザではマイクを使用できません。\n" +
      "ブラウザは暗号化されていない接続（http://）でのマイク使用を禁止しています。\n\n" +
      "対処方法:\n" +
      "・サーバを動かしているPC本体では http://localhost:8787 を開いてください（マイクを使えます）\n" +
      "・他のPCから録音する場合は、HTTPS を有効にする必要があります"
    );
  }
  return "このブラウザはマイク入力に対応していません。";
}

/** 録音音声の再生URL。 */
export function audioSourceUrl(meetingId: string, audioPath: string): string {
  if (isDesktop) {
    // Tauri の asset プロトコル経由でローカルファイルを再生する
    // 動的 import を避けるため、呼び出し側で convertFileSrc を渡す設計にはしない。
    return audioPath;
  }
  return `/api/audio/${encodeURIComponent(meetingId)}`;
}
