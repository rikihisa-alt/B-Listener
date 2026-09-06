//! 会議まわりの CRUD。
//!
//! ここに SQL を集約し、上位層（commands / pipeline）は SQL を書かない。

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, AppResult};

use super::models::*;
use super::{new_id, now_iso8601};

const MEETING_COLUMNS: &str =
    "id, title, scheduled_at, started_at, ended_at, duration_ms, status, \
     folder_path, audio_path, audio_format, sample_rate, transcript_path, minutes_path, \
     summary_path, goal, carryover, notes, created_at, updated_at";

fn row_to_meeting(row: &Row) -> rusqlite::Result<Meeting> {
    Ok(Meeting {
        id: row.get(0)?,
        title: row.get(1)?,
        scheduled_at: row.get(2)?,
        started_at: row.get(3)?,
        ended_at: row.get(4)?,
        duration_ms: row.get(5)?,
        status: MeetingStatus::parse(&row.get::<_, String>(6)?),
        folder_path: row.get(7)?,
        audio_path: row.get(8)?,
        audio_format: row.get(9)?,
        sample_rate: row.get(10)?,
        transcript_path: row.get(11)?,
        minutes_path: row.get(12)?,
        summary_path: row.get(13)?,
        goal: row.get(14)?,
        carryover: row.get(15)?,
        notes: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

// ---------------------------------------------------------------- meeting

/// 新しい会議を `draft` 状態で作成する。会議名が空なら日時から自動生成する。
pub fn create_meeting(conn: &Connection, title: Option<String>) -> AppResult<Meeting> {
    let now = now_iso8601();
    let id = new_id();
    let title = normalize_title(title);

    conn.execute(
        "INSERT INTO meeting (id, title, duration_ms, status, goal, carryover, notes, created_at, updated_at)
         VALUES (?1, ?2, 0, ?3, '', '', '', ?4, ?4)",
        params![id, title, MeetingStatus::Draft.as_str(), now],
    )?;

    tracing::info!(meeting_id = %id, "会議を作成しました");
    get_meeting(conn, &id)
}

fn normalize_title(title: Option<String>) -> String {
    let trimmed = title.unwrap_or_default().trim().to_string();
    if trimmed.is_empty() {
        chrono::Local::now()
            .format("会議 %Y-%m-%d %H:%M")
            .to_string()
    } else {
        trimmed
    }
}

pub fn get_meeting(conn: &Connection, id: &str) -> AppResult<Meeting> {
    let sql = format!("SELECT {MEETING_COLUMNS} FROM meeting WHERE id = ?1");
    conn.query_row(&sql, params![id], row_to_meeting)
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("会議 {id}")))
}

pub fn list_meetings(conn: &Connection, limit: i64) -> AppResult<Vec<MeetingListItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, started_at, created_at, duration_ms, status,
                audio_path IS NOT NULL, minutes_path IS NOT NULL, summary_path IS NOT NULL
         FROM meeting
         ORDER BY COALESCE(started_at, created_at) DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(MeetingListItem {
            id: row.get(0)?,
            title: row.get(1)?,
            started_at: row.get(2)?,
            created_at: row.get(3)?,
            duration_ms: row.get(4)?,
            status: MeetingStatus::parse(&row.get::<_, String>(5)?),
            has_audio: row.get(6)?,
            has_minutes: row.get(7)?,
            has_summary: row.get(8)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 起動時のクラッシュ復旧対象（`recording` / `paused` のまま残っている会議）。
pub fn list_interrupted_meetings(conn: &Connection) -> AppResult<Vec<Meeting>> {
    let sql = format!(
        "SELECT {MEETING_COLUMNS} FROM meeting
         WHERE status IN ('recording','paused')
         ORDER BY created_at ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_meeting)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn set_meeting_status(conn: &Connection, id: &str, status: MeetingStatus) -> AppResult<()> {
    let changed = conn.execute(
        "UPDATE meeting SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, status.as_str(), now_iso8601()],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("会議 {id}")));
    }
    Ok(())
}

pub fn delete_meeting(conn: &Connection, id: &str) -> AppResult<()> {
    let changed = conn.execute("DELETE FROM meeting WHERE id = ?1", params![id])?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("会議 {id}")));
    }
    tracing::info!(meeting_id = %id, "会議レコードを削除しました");
    Ok(())
}

// ------------------------------------------------------- 事前情報の保存

/// 事前入力を保存する。すべて任意項目で、`None` の項目は変更しない。
///
/// 参加者・議題・用語は「まとめて置き換える」。部分更新にすると UI 側の
/// 差分管理が複雑になり、消え残りの原因になるため。
pub fn save_meeting_context(
    tx: &rusqlite::Transaction,
    meeting_id: &str,
    input: &MeetingContextInput,
) -> AppResult<()> {
    // 会議が存在することを先に確認する（外部キー違反より分かりやすいエラーにする）。
    let exists: bool = tx
        .query_row(
            "SELECT 1 FROM meeting WHERE id = ?1",
            params![meeting_id],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Err(AppError::NotFound(format!("会議 {meeting_id}")));
    }

    if let Some(title) = &input.title {
        let title = normalize_title(Some(title.clone()));
        tx.execute(
            "UPDATE meeting SET title = ?2 WHERE id = ?1",
            params![meeting_id, title],
        )?;
    }
    for (column, value) in [
        ("scheduled_at", input.scheduled_at.as_ref()),
        ("goal", input.goal.as_ref()),
        ("carryover", input.carryover.as_ref()),
        ("notes", input.notes.as_ref()),
    ] {
        if let Some(v) = value {
            tx.execute(
                &format!("UPDATE meeting SET {column} = ?2 WHERE id = ?1"),
                params![meeting_id, v],
            )?;
        }
    }
    tx.execute(
        "UPDATE meeting SET updated_at = ?2 WHERE id = ?1",
        params![meeting_id, now_iso8601()],
    )?;

    if let Some(participants) = &input.participants {
        tx.execute(
            "DELETE FROM participant WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        for (i, name) in participants
            .iter()
            .filter(|s| !s.trim().is_empty())
            .enumerate()
        {
            tx.execute(
                "INSERT INTO participant (id, meeting_id, name, sort_order) VALUES (?1, ?2, ?3, ?4)",
                params![new_id(), meeting_id, name.trim(), i as i64],
            )?;
        }
    }

    if let Some(agendas) = &input.agendas {
        tx.execute(
            "DELETE FROM agenda WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        for (i, text) in agendas.iter().filter(|s| !s.trim().is_empty()).enumerate() {
            tx.execute(
                "INSERT INTO agenda (id, meeting_id, text, sort_order) VALUES (?1, ?2, ?3, ?4)",
                params![new_id(), meeting_id, text.trim(), i as i64],
            )?;
        }
    }

    if let Some(terms) = &input.terms {
        tx.execute(
            "DELETE FROM context_term WHERE meeting_id = ?1",
            params![meeting_id],
        )?;
        for (i, t) in terms
            .iter()
            .filter(|t| !t.term.trim().is_empty())
            .enumerate()
        {
            tx.execute(
                "INSERT INTO context_term (id, meeting_id, term, category, reading, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    new_id(),
                    meeting_id,
                    t.term.trim(),
                    t.category.as_str(),
                    t.reading.trim(),
                    i as i64
                ],
            )?;
        }
    }

    Ok(())
}

pub fn list_participants(conn: &Connection, meeting_id: &str) -> AppResult<Vec<Participant>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, sort_order FROM participant WHERE meeting_id = ?1 ORDER BY sort_order",
    )?;
    let rows = stmt.query_map(params![meeting_id], |row| {
        Ok(Participant {
            id: row.get(0)?,
            name: row.get(1)?,
            sort_order: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn list_agendas(conn: &Connection, meeting_id: &str) -> AppResult<Vec<Agenda>> {
    let mut stmt = conn.prepare(
        "SELECT id, text, sort_order FROM agenda WHERE meeting_id = ?1 ORDER BY sort_order",
    )?;
    let rows = stmt.query_map(params![meeting_id], |row| {
        Ok(Agenda {
            id: row.get(0)?,
            text: row.get(1)?,
            sort_order: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn list_terms(conn: &Connection, meeting_id: &str) -> AppResult<Vec<ContextTerm>> {
    let mut stmt = conn.prepare(
        "SELECT id, term, category, reading, sort_order
         FROM context_term WHERE meeting_id = ?1 ORDER BY sort_order",
    )?;
    let rows = stmt.query_map(params![meeting_id], |row| {
        Ok(ContextTerm {
            id: row.get(0)?,
            term: row.get(1)?,
            category: TermCategory::parse(&row.get::<_, String>(2)?),
            reading: row.get(3)?,
            sort_order: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_meeting_detail(conn: &Connection, meeting_id: &str) -> AppResult<MeetingDetail> {
    Ok(MeetingDetail {
        meeting: get_meeting(conn, meeting_id)?,
        participants: list_participants(conn, meeting_id)?,
        agendas: list_agendas(conn, meeting_id)?,
        terms: list_terms(conn, meeting_id)?,
    })
}

// ------------------------------------------------------------ 録音の記録

/// 録音開始をDBへ記録する。
///
/// 重要: 録音を実際に開始する **前** に呼ぶこと。
/// 開始直後にクラッシュしても「復旧対象の会議」として検出できるようにするため。
pub fn mark_recording_started(
    conn: &Connection,
    meeting_id: &str,
    folder_path: &str,
    audio_path: &str,
    sample_rate: i64,
) -> AppResult<Meeting> {
    let now = now_iso8601();
    let changed = conn.execute(
        "UPDATE meeting
         SET status = ?2, started_at = ?3, folder_path = ?4, audio_path = ?5,
             audio_format = 'wav', sample_rate = ?6, updated_at = ?3
         WHERE id = ?1",
        params![
            meeting_id,
            MeetingStatus::Recording.as_str(),
            now,
            folder_path,
            audio_path,
            sample_rate
        ],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("会議 {meeting_id}")));
    }
    get_meeting(conn, meeting_id)
}

/// 録音終了をDBへ記録する。
pub fn mark_recording_finished(
    conn: &Connection,
    meeting_id: &str,
    duration_ms: i64,
    status: MeetingStatus,
) -> AppResult<Meeting> {
    let now = now_iso8601();
    let changed = conn.execute(
        "UPDATE meeting
         SET status = ?2, ended_at = ?3, duration_ms = ?4, updated_at = ?3
         WHERE id = ?1",
        params![meeting_id, status.as_str(), now, duration_ms],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound(format!("会議 {meeting_id}")));
    }
    get_meeting(conn, meeting_id)
}

/// 復旧処理で音声ファイルのパスが変わった場合に更新する。
pub fn set_audio_path(conn: &Connection, meeting_id: &str, audio_path: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE meeting SET audio_path = ?2, updated_at = ?3 WHERE id = ?1",
        params![meeting_id, audio_path, now_iso8601()],
    )?;
    Ok(())
}

// -------------------------------------------------------- 文字起こしの保存

/// 指定種別の文字起こしを入れ替える。
///
/// 再実行時に古い結果が混ざらないよう、常に「削除してから挿入」する。
pub fn replace_transcript_segments(
    tx: &rusqlite::Transaction,
    meeting_id: &str,
    kind: TranscriptKind,
    segments: &[crate::stt::Segment],
) -> AppResult<()> {
    tx.execute(
        "DELETE FROM transcript_segment WHERE meeting_id = ?1 AND kind = ?2",
        params![meeting_id, kind.as_str()],
    )?;

    let now = now_iso8601();
    let mut stmt = tx.prepare(
        "INSERT INTO transcript_segment
             (id, meeting_id, kind, seq, start_ms, end_ms, text, raw_text, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for (i, seg) in segments.iter().enumerate() {
        stmt.execute(params![
            new_id(),
            meeting_id,
            kind.as_str(),
            i as i64,
            seg.start_ms,
            seg.end_ms,
            seg.text,
            seg.text,
            now
        ])?;
    }
    Ok(())
}

pub fn list_transcript_segments(
    conn: &Connection,
    meeting_id: &str,
    kind: TranscriptKind,
) -> AppResult<Vec<TranscriptSegment>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, seq, start_ms, end_ms, text, raw_text
         FROM transcript_segment
         WHERE meeting_id = ?1 AND kind = ?2
         ORDER BY seq",
    )?;
    let rows = stmt.query_map(params![meeting_id, kind.as_str()], |row| {
        Ok(TranscriptSegment {
            id: row.get(0)?,
            kind: if row.get::<_, String>(1)? == "final" {
                TranscriptKind::Final
            } else {
                TranscriptKind::Realtime
            },
            seq: row.get(2)?,
            start_ms: row.get(3)?,
            end_ms: row.get(4)?,
            text: row.get(5)?,
            raw_text: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn set_transcript_path(conn: &Connection, meeting_id: &str, path: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE meeting SET transcript_path = ?2, updated_at = ?3 WHERE id = ?1",
        params![meeting_id, path, now_iso8601()],
    )?;
    Ok(())
}

// ------------------------------------------------------------ パイプライン

/// ステップの状態を記録する。同じステップは 1 行に集約する。
pub fn set_job_state(
    conn: &Connection,
    meeting_id: &str,
    step: PipelineStep,
    state: JobState,
    error: Option<&str>,
) -> AppResult<()> {
    let now = now_iso8601();
    let started_at = if state == JobState::Running {
        Some(now.clone())
    } else {
        None
    };
    let finished_at = if matches!(state, JobState::Done | JobState::Failed) {
        Some(now.clone())
    } else {
        None
    };

    conn.execute(
        "INSERT INTO job_run (id, meeting_id, step, state, attempt, error, started_at, finished_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7)
         ON CONFLICT (meeting_id, step) DO UPDATE SET
             state       = excluded.state,
             error       = excluded.error,
             attempt     = job_run.attempt + CASE WHEN excluded.state = 'running' THEN 1 ELSE 0 END,
             started_at  = COALESCE(excluded.started_at, job_run.started_at),
             finished_at = excluded.finished_at",
        params![
            new_id(),
            meeting_id,
            step.as_str(),
            state.as_str(),
            error,
            started_at,
            finished_at
        ],
    )?;
    Ok(())
}

pub fn list_job_runs(conn: &Connection, meeting_id: &str) -> AppResult<Vec<JobRun>> {
    let mut stmt = conn.prepare(
        "SELECT step, state, attempt, error, started_at, finished_at
         FROM job_run WHERE meeting_id = ?1",
    )?;
    let rows = stmt.query_map(params![meeting_id], |row| {
        let step_text: String = row.get(0)?;
        Ok((
            step_text,
            JobState::parse(&row.get::<_, String>(1)?),
            row.get::<_, i64>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (step_text, state, attempt, error, started_at, finished_at) = row?;
        // 未知のステップ名（将来のバージョンで書かれた行など）は無視する。
        let Some(step) = PipelineStep::parse(&step_text) else {
            continue;
        };
        out.push(JobRun {
            step,
            label: step.label().to_string(),
            state,
            attempt,
            error,
            started_at,
            finished_at,
        });
    }
    Ok(out)
}

/// 会議に紐づく全ての事前情報用語を、文字起こし補正用に平坦な一覧として返す。
pub fn list_all_term_strings(conn: &Connection, meeting_id: &str) -> AppResult<Vec<String>> {
    let mut out: Vec<String> = list_participants(conn, meeting_id)?
        .into_iter()
        .map(|p| p.name)
        .collect();
    out.extend(list_terms(conn, meeting_id)?.into_iter().map(|t| t.term));
    Ok(out)
}

/// 指定ステップの記録を消し、次回の実行でやり直させる。
pub fn clear_job_runs(conn: &Connection, meeting_id: &str, steps: &[&str]) -> AppResult<()> {
    for step in steps {
        conn.execute(
            "DELETE FROM job_run WHERE meeting_id = ?1 AND step = ?2",
            params![meeting_id, step],
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------- 集計

/// ホーム画面に出す件数。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeStats {
    /// 今月に開始した会議の件数。
    pub meetings_this_month: i64,
    /// 会議終了後の処理がまだ終わっていない件数（処理中・失敗）。
    pub unprocessed: i64,
    /// 録音ファイルが保存されている会議の件数。
    pub saved_audio: i64,
}

pub fn home_stats(conn: &Connection) -> AppResult<HomeStats> {
    // 月の境界はローカルタイムで判定する。DB には ISO8601 の文字列で入っているため、
    // 「YYYY-MM」の前方一致で数えるのが最も素直で取りこぼしがない。
    let month_prefix = chrono::Local::now().format("%Y-%m").to_string();

    let meetings_this_month: i64 = conn.query_row(
        "SELECT COUNT(*) FROM meeting
         WHERE substr(COALESCE(started_at, created_at), 1, 7) = ?1",
        params![month_prefix],
        |row| row.get(0),
    )?;

    let unprocessed: i64 = conn.query_row(
        "SELECT COUNT(*) FROM meeting WHERE status IN ('processing','failed','recording','paused')",
        [],
        |row| row.get(0),
    )?;

    let saved_audio: i64 = conn.query_row(
        "SELECT COUNT(*) FROM meeting WHERE audio_path IS NOT NULL",
        [],
        |row| row.get(0),
    )?;

    Ok(HomeStats {
        meetings_this_month,
        unprocessed,
        saved_audio,
    })
}
