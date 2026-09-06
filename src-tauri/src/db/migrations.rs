//! スキーマ定義とマイグレーション。
//!
//! `schema_version` テーブルで適用済みバージョンを管理し、起動時に前方適用する。
//! 各マイグレーションは 1 トランザクションで実行し、途中失敗時はロールバックする。

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

/// (version, SQL) の並び。追加は必ず末尾へ行い、既存の SQL は変更しない。
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("sql/001_init.sql"))];

pub fn run(conn: &mut Connection) -> AppResult<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS schema_version (
             version    INTEGER PRIMARY KEY,
             applied_at TEXT NOT NULL
         );",
    )
    .map_err(|e| AppError::Db(format!("初期設定に失敗しました: {e}")))?;

    let applied: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Db(format!("スキーマバージョンを取得できません: {e}")))?;

    for (version, sql) in MIGRATIONS {
        if *version <= applied {
            continue;
        }
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Db(format!("トランザクションを開始できません: {e}")))?;
        tx.execute_batch(sql).map_err(|e| {
            AppError::Db(format!(
                "マイグレーション v{version} の適用に失敗しました: {e}"
            ))
        })?;
        tx.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, crate::db::now_iso8601()],
        )
        .map_err(|e| AppError::Db(format!("スキーマバージョンを記録できません: {e}")))?;
        tx.commit()
            .map_err(|e| AppError::Db(format!("マイグレーションを確定できません: {e}")))?;

        tracing::info!(version, "スキーマを適用しました");
    }

    Ok(())
}
