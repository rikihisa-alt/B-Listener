//! SQLite アクセス層。
//!
//! DB は「検索・一覧のためのインデックス」であり、データの唯一の正は会議フォルダ内の
//! ファイル群（audio.wav / transcript.txt / minutes.md / summary.md / metadata.json）である。

pub mod migrations;
pub mod models;
pub mod repo;

use std::path::Path;
use std::sync::Mutex;

use chrono::Local;
use rusqlite::Connection;

use crate::error::{AppError, AppResult};

/// DB へのアクセスを直列化するラッパ。
///
/// 会議中の書き込みは秒間数件程度で競合しないため、単一接続 + Mutex で十分。
/// WAL モードのため UI からの読み取りが書き込みでブロックされることもない。
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Db(format!("DBフォルダを作成できません: {e}")))?;
        }

        let mut conn = Connection::open(path)
            .map_err(|e| AppError::Db(format!("DBを開けません ({}): {e}", path.display())))?;

        migrations::run(&mut conn)?;

        tracing::info!(path = %path.display(), "データベースを開きました");
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 読み取り・単発書き込み用。
    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| AppError::Db("DB接続の取得に失敗しました".into()))?;
        f(&guard)
    }

    /// 複数テーブルにまたがる更新用。クロージャが Err を返せばロールバックする。
    pub fn with_tx<T>(
        &self,
        f: impl FnOnce(&rusqlite::Transaction) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| AppError::Db("DB接続の取得に失敗しました".into()))?;
        let tx = guard
            .transaction()
            .map_err(|e| AppError::Db(format!("トランザクションを開始できません: {e}")))?;
        let result = f(&tx)?;
        tx.commit()
            .map_err(|e| AppError::Db(format!("変更を確定できません: {e}")))?;
        Ok(result)
    }
}

/// DB へ保存する日時の共通形式（ローカルタイムゾーンのオフセット付き ISO8601）。
pub fn now_iso8601() -> String {
    Local::now().to_rfc3339()
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
