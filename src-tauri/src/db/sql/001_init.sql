-- B-Listener 初期スキーマ
-- 設計の詳細は docs/02-database.md を参照。

CREATE TABLE meeting (
    id              TEXT PRIMARY KEY,
    title           TEXT NOT NULL,
    scheduled_at    TEXT,
    started_at      TEXT,
    ended_at        TEXT,
    duration_ms     INTEGER NOT NULL DEFAULT 0,
    status          TEXT NOT NULL,           -- draft|recording|paused|processing|completed|failed
    folder_path     TEXT,
    audio_path      TEXT,
    audio_format    TEXT,
    sample_rate     INTEGER,
    transcript_path TEXT,
    minutes_path    TEXT,
    summary_path    TEXT,
    goal            TEXT NOT NULL DEFAULT '',
    carryover       TEXT NOT NULL DEFAULT '',
    notes           TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX idx_meeting_created ON meeting (created_at DESC);
CREATE INDEX idx_meeting_status  ON meeting (status);

CREATE TABLE participant (
    id         TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meeting(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_participant_meeting ON participant (meeting_id, sort_order);

CREATE TABLE agenda (
    id         TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meeting(id) ON DELETE CASCADE,
    text       TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_agenda_meeting ON agenda (meeting_id, sort_order);

-- 事前入力の AI 補助情報（文字起こし補正と LLM のコンテキストに使用）
CREATE TABLE context_term (
    id         TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meeting(id) ON DELETE CASCADE,
    term       TEXT NOT NULL,
    category   TEXT NOT NULL,   -- person|user|customer|company|service|jargon|abbrev|other
    reading    TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_term_meeting ON context_term (meeting_id, category, sort_order);

-- kind でリアルタイム結果と最終結果を分離する。
-- 最終議事録は kind='final' のみを参照し、リアルタイム結果の混入を防ぐ。
CREATE TABLE transcript_segment (
    id         TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meeting(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,   -- realtime|final
    seq        INTEGER NOT NULL,
    start_ms   INTEGER NOT NULL,
    end_ms     INTEGER NOT NULL,
    text       TEXT NOT NULL,
    raw_text   TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);
CREATE INDEX idx_segment_meeting ON transcript_segment (meeting_id, kind, seq);

-- 決定事項・注意事項・保留・次回確認・重要ポイント・議題は同型のため 1 テーブルに統合する。
CREATE TABLE meeting_note (
    id           TEXT PRIMARY KEY,
    meeting_id   TEXT NOT NULL REFERENCES meeting(id) ON DELETE CASCADE,
    kind         TEXT NOT NULL,  -- decision|warning|pending|next_check|important|topic
    text         TEXT NOT NULL,
    source       TEXT NOT NULL,  -- realtime|final
    chunk_idx    INTEGER,
    needs_review INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_note_meeting ON meeting_note (meeting_id, kind, created_at);

-- 仕様書の Decision テーブルに相当する互換ビュー
CREATE VIEW decision AS
    SELECT id, meeting_id, text, source, chunk_idx, created_at
    FROM meeting_note WHERE kind = 'decision';

CREATE TABLE action_item (
    id           TEXT PRIMARY KEY,
    meeting_id   TEXT NOT NULL REFERENCES meeting(id) ON DELETE CASCADE,
    person       TEXT NOT NULL DEFAULT '',   -- 空文字 = 担当者未定
    action       TEXT NOT NULL,
    deadline     TEXT NOT NULL DEFAULT '',   -- 空文字 = 期限未定
    deadline_raw TEXT NOT NULL DEFAULT '',   -- 発言そのままの表現
    status       TEXT NOT NULL DEFAULT 'open',  -- open|done|dropped
    source       TEXT NOT NULL,              -- realtime|final
    chunk_idx    INTEGER,
    needs_review INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_action_meeting ON action_item (meeting_id, created_at);

CREATE TABLE analysis_snapshot (
    id         TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL REFERENCES meeting(id) ON DELETE CASCADE,
    at_ms      INTEGER NOT NULL,
    payload    TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_snapshot_meeting ON analysis_snapshot (meeting_id, at_ms);

-- 終了後パイプラインの進捗。途中失敗しても完了済みステップをやり直さない。
CREATE TABLE job_run (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL REFERENCES meeting(id) ON DELETE CASCADE,
    step        TEXT NOT NULL,
    state       TEXT NOT NULL,   -- pending|running|done|failed
    attempt     INTEGER NOT NULL DEFAULT 0,
    error       TEXT,
    started_at  TEXT,
    finished_at TEXT,
    UNIQUE (meeting_id, step)
);
CREATE INDEX idx_job_meeting ON job_run (meeting_id);
