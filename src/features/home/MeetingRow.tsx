import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/Feedback";
import { Icon, IconBadge } from "@/components/ui/Icon";
import { formatDate, formatDurationJa } from "@/lib/format";
import type { MeetingListItem } from "@/types/ipc";

import { STATUS_LABEL, statusTone } from "./meetingStatus";
import "./home.css";

/** 会議1件の行。ホームの「最近の会議」と会議一覧で共用する。 */
export function MeetingRow({ meeting }: { meeting: MeetingListItem }) {
  const subtitle = buildSubtitle(meeting);

  return (
    <Link to={`/meetings/${meeting.id}`} className="meeting-row">
      <IconBadge name="document" tone={meeting.status === "completed" ? "blue" : "orange"} />

      <span className="meeting-row__main">
        <span className="meeting-row__title truncate">{meeting.title}</span>
        {subtitle && <span className="meeting-row__sub text-sm text-muted truncate">{subtitle}</span>}
      </span>

      <span className="meeting-row__meta text-sm text-muted">
        <Icon name="calendar" size={15} />
        <span className="tabular">{formatDate(meeting.startedAt ?? meeting.createdAt)}</span>
      </span>

      <span className="meeting-row__meta text-sm text-muted">
        <Icon name="clock" size={15} />
        <span className="tabular">
          {meeting.durationMs > 0 ? formatDurationJa(meeting.durationMs) : "—"}
        </span>
      </span>

      <Badge tone={statusTone(meeting.status)}>{STATUS_LABEL[meeting.status]}</Badge>
    </Link>
  );
}

function buildSubtitle(meeting: MeetingListItem): string {
  const parts: string[] = [];
  if (meeting.hasAudio) parts.push("音声");
  if (meeting.hasMinutes) parts.push("議事録");
  if (meeting.hasSummary) parts.push("AIまとめ");
  if (parts.length === 0) return "";
  return parts.join(" ・ ");
}
