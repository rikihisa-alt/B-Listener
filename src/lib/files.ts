/**
 * 成果物の再生・保存・フォルダを開く操作。
 *
 * デスクトップ版は OS のダイアログとファイルシステムを使い、
 * ブラウザ版はサーバからのダウンロードで代替する。
 * 画面側はこのモジュールの関数だけを呼ぶ。
 */

import { convertFileSrc } from "@tauri-apps/api/core";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { exportMeetingAudio, exportMeetingDocument } from "./ipc";
import { isDesktop } from "./runtime";
import type { MeetingDocument } from "@/types/ipc";

/** 録音音声の再生に使う URL。 */
export function audioUrl(meetingId: string, audioPath: string): string {
  if (isDesktop) {
    return convertFileSrc(audioPath);
  }
  return `/api/audio/${encodeURIComponent(meetingId)}`;
}

/** 保存フォルダを開けるか（ブラウザ版では利用者のPCのフォルダを開けない）。 */
export const canRevealInFolder = isDesktop;

/** 保存フォルダを開く（デスクトップ版のみ）。 */
export async function revealInFolder(path: string): Promise<void> {
  if (!isDesktop) return;
  await revealItemInDir(path);
}

/** 議事録・まとめ・文字起こしを保存する。 */
export async function saveDocument(
  meetingId: string,
  document: MeetingDocument,
  defaultName: string,
): Promise<void> {
  if (isDesktop) {
    const target = await saveDialog({ defaultPath: defaultName });
    if (!target) return;
    await exportMeetingDocument(meetingId, document, target);
    return;
  }
  triggerBrowserDownload(`/api/download/${encodeURIComponent(meetingId)}/${document}`);
}

/** 録音音声を保存する。 */
export async function saveAudio(meetingId: string): Promise<void> {
  if (isDesktop) {
    const target = await saveDialog({ defaultPath: "audio.wav" });
    if (!target) return;
    await exportMeetingAudio(meetingId, target);
    return;
  }
  triggerBrowserDownload(`/api/download/${encodeURIComponent(meetingId)}/audio`);
}

function triggerBrowserDownload(url: string) {
  const anchor = document.createElement("a");
  anchor.href = url;
  // ファイル名はサーバの Content-Disposition に従わせる
  anchor.rel = "noopener";
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
}
