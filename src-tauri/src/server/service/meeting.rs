//! （`commands/meeting.rs` から呼ばれる共有ロジック。Tauri にもHTTPサーバにも依存しない）
//! 会議の作成・取得・更新・削除。

use crate::db::models::{MeetingContextInput, MeetingDetail, MeetingListItem};
use crate::db::{repo, Database};
use crate::error::AppResult;
use crate::state::AppState;

/// 一覧の最大取得件数。UI 側のページングは MVP では行わない。
const MEETING_LIST_LIMIT: i64 = 500;

pub fn create_meeting(
    state: &AppState,
    title: Option<String>,
) -> AppResult<crate::db::models::Meeting> {
    state.db.with_conn(|conn| repo::create_meeting(conn, title))
}

/// ホーム画面の集計値。
pub fn get_home_stats(state: &AppState) -> AppResult<repo::HomeStats> {
    state.db.with_conn(repo::home_stats)
}

pub fn list_meetings(state: &AppState) -> AppResult<Vec<MeetingListItem>> {
    state
        .db
        .with_conn(|conn| repo::list_meetings(conn, MEETING_LIST_LIMIT))
}

pub fn get_meeting_detail(state: &AppState, meeting_id: String) -> AppResult<MeetingDetail> {
    state
        .db
        .with_conn(|conn| repo::get_meeting_detail(conn, &meeting_id))
}

/// 事前情報を保存する。すべて任意項目であり、未指定の項目は変更しない。
pub fn save_meeting_context(
    state: &AppState,
    meeting_id: String,
    input: MeetingContextInput,
) -> AppResult<MeetingDetail> {
    let db: &Database = &state.db;
    db.with_tx(|tx| repo::save_meeting_context(tx, &meeting_id, &input))?;
    db.with_conn(|conn| repo::get_meeting_detail(conn, &meeting_id))
}

/// 会議レコードを削除する。
///
/// 注意: 録音ファイルを含む会議フォルダは削除しない。
/// 「録音データを絶対に失わない」方針のため、フォルダの削除は利用者が明示的に行う。
pub fn delete_meeting(state: &AppState, meeting_id: String) -> AppResult<()> {
    state
        .db
        .with_conn(|conn| repo::delete_meeting(conn, &meeting_id))
}
