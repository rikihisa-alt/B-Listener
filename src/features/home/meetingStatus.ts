import type { MeetingStatus } from "@/types/ipc";

export const STATUS_LABEL: Record<MeetingStatus, string> = {
  draft: "未開始",
  recording: "録音中",
  paused: "一時停止",
  processing: "処理中",
  completed: "完了",
  failed: "要確認",
};

export type Tone = "neutral" | "ok" | "warn" | "danger" | "info";

export function statusTone(status: MeetingStatus): Tone {
  switch (status) {
    case "completed":
      return "ok";
    case "recording":
    case "paused":
      return "danger";
    case "processing":
      return "warn";
    case "failed":
      return "warn";
    default:
      return "neutral";
  }
}
