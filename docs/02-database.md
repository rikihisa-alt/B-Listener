# 02. DB設計（SQLite）

## 位置づけ

DB は **検索・一覧のためのインデックス**です。データの唯一の正は会議フォルダ内のファイル
（`audio.wav` / `transcript.txt` / `minutes.md` / `summary.md` / `metadata.json`）です。
DB が破損しても各フォルダの `metadata.json` から再構築できます。

- 保存先: `<AppData>/B-Listener/app.db`
- 接続: WAL モード（会議中の書き込みと UI 読み取りが競合しないため）
- マイグレーション: `schema_version` テーブルで管理し、起動時に前方適用

---

## ER 概要

```
meeting 1─┬─* participant
          ├─* agenda
          ├─* context_term
          ├─* transcript_segment
          ├─* meeting_note
          ├─* action_item
          ├─* analysis_snapshot
          └─* job_run
```

---

## スキーマ

### meeting

会議1件。仕様の必須項目に加え、事前入力とパイプライン状態を保持します。

| カラム | 型 | 説明 |
|---|---|---|
| id | TEXT PK | UUID v4 |
| title | TEXT NOT NULL | 会議名（未入力時は「会議 YYYY-MM-DD HH:MM」を自動生成） |
| scheduled_at | TEXT | 事前入力の会議日時（ISO8601） |
| started_at | TEXT | 録音開始時刻 |
| ended_at | TEXT | 録音終了時刻 |
| duration_ms | INTEGER | 実録音時間（一時停止分を除く） |
| status | TEXT NOT NULL | `draft` / `recording` / `paused` / `processing` / `completed` / `failed` |
| folder_path | TEXT | 会議フォルダの絶対パス |
| audio_path | TEXT | 音声ファイル |
| audio_format | TEXT | `wav` / `m4a` |
| sample_rate | INTEGER | 16000 |
| transcript_path | TEXT | 最終文字起こし |
| minutes_path | TEXT | 議事録 |
| summary_path | TEXT | AIまとめ |
| goal | TEXT | 事前入力「今回決めたいこと」 |
| carryover | TEXT | 事前入力「前回からの持ち越し事項」 |
| notes | TEXT | 事前入力「補足情報」 |
| created_at | TEXT NOT NULL | |
| updated_at | TEXT NOT NULL | |

`status` が `recording` / `paused` のままアプリが起動した場合、クラッシュとみなして復旧処理へ入ります。

### participant

| カラム | 型 | 説明 |
|---|---|---|
| id | TEXT PK | |
| meeting_id | TEXT NOT NULL | FK → meeting.id ON DELETE CASCADE |
| name | TEXT NOT NULL | |
| sort_order | INTEGER NOT NULL | |

参加者名は文字起こし補正の辞書としても使われます。

### agenda

| カラム | 型 | 説明 |
|---|---|---|
| id | TEXT PK | |
| meeting_id | TEXT NOT NULL | FK |
| text | TEXT NOT NULL | 議題1件 |
| sort_order | INTEGER NOT NULL | |

### context_term

事前入力の AI補助情報。文字起こし補正と LLM のコンテキストに使います。

| カラム | 型 | 説明 |
|---|---|---|
| id | TEXT PK | |
| meeting_id | TEXT NOT NULL | FK |
| term | TEXT NOT NULL | 例: `国保連`, `A勤` |
| category | TEXT NOT NULL | `person` / `user` / `customer` / `company` / `service` / `jargon` / `abbrev` / `other` |
| reading | TEXT | 任意のよみ（補正の精度向上用） |

### transcript_segment

文字起こしをセグメント単位で保持します。リアルタイム結果と最終結果を `kind` で分離し、
**リアルタイム結果が最終議事録に混入しないこと**を DB レベルで保証します。

| カラム | 型 | 説明 |
|---|---|---|
| id | TEXT PK | |
| meeting_id | TEXT NOT NULL | FK |
| kind | TEXT NOT NULL | `realtime` / `final` |
| seq | INTEGER NOT NULL | 会議内の連番 |
| start_ms | INTEGER NOT NULL | 録音開始からのオフセット |
| end_ms | INTEGER NOT NULL | |
| text | TEXT NOT NULL | 補正後テキスト |
| raw_text | TEXT | 補正前テキスト（補正の妥当性を後から検証するため） |
| created_at | TEXT NOT NULL | |

インデックス: `(meeting_id, kind, seq)`

### meeting_note

決定事項・注意事項・保留・次回確認・重要ポイント・議題は、いずれも
「会議から抽出された1行のテキスト」という同じ構造なので 1 テーブルに統合します。

| カラム | 型 | 説明 |
|---|---|---|
| id | TEXT PK | |
| meeting_id | TEXT NOT NULL | FK |
| kind | TEXT NOT NULL | `decision` / `warning` / `pending` / `next_check` / `important` / `topic` |
| text | TEXT NOT NULL | |
| source | TEXT NOT NULL | `realtime` / `final` |
| chunk_idx | INTEGER | 抽出元チャンク番号（長時間会議の追跡用） |
| created_at | TEXT NOT NULL | |

> 仕様書の `Decision` テーブルは `kind='decision'` に相当します。
> 互換のため `decision` という VIEW を用意します。

### action_item

ネクストアクションは構造が異なる（担当者・期限を持つ）ため独立テーブルにします。

| カラム | 型 | 説明 |
|---|---|---|
| id | TEXT PK | |
| meeting_id | TEXT NOT NULL | FK |
| person | TEXT | 空文字なら「担当者未定」 |
| action | TEXT NOT NULL | |
| deadline | TEXT | 空文字なら「期限未定」 |
| deadline_raw | TEXT | 発言そのままの表現（例:「火曜まで」）。日付への変換で意味を失わないため |
| status | TEXT NOT NULL | `open` / `done` / `dropped` |
| source | TEXT NOT NULL | `realtime` / `final` |
| chunk_idx | INTEGER | |
| created_at | TEXT NOT NULL | |

**チャンク処理で消失させない仕組み**: 長時間会議のチャンク要約では、各チャンクから抽出した
決定事項・ネクストアクションを**その場で `meeting_note` / `action_item` に永続化**します。
後段の統合処理はこの構造化データに対して重複排除を行うだけで、新規の消失は起きません。

### analysis_snapshot

会議中のリアルタイム分析の履歴。会議中に AI が出した内容を後から追跡できます。

| カラム | 型 | 説明 |
|---|---|---|
| id | TEXT PK | |
| meeting_id | TEXT NOT NULL | FK |
| at_ms | INTEGER NOT NULL | 録音開始からのオフセット |
| payload | TEXT NOT NULL | 分析結果 JSON |
| created_at | TEXT NOT NULL | |

### job_run

終了後パイプラインの各ステップ状態。途中失敗しても続きから再開するために使います。

| カラム | 型 | 説明 |
|---|---|---|
| id | TEXT PK | |
| meeting_id | TEXT NOT NULL | FK |
| step | TEXT NOT NULL | `finalize_audio` / `transcribe` / `correct` / `extract` / `merge` / `minutes` / `summary` / `persist` |
| state | TEXT NOT NULL | `pending` / `running` / `done` / `failed` |
| attempt | INTEGER NOT NULL | |
| error | TEXT | 失敗理由（握り潰さない） |
| started_at | TEXT | |
| finished_at | TEXT | |

一意制約: `(meeting_id, step)`

### schema_version

| カラム | 型 |
|---|---|
| version | INTEGER PK |
| applied_at | TEXT NOT NULL |

---

## アプリ設定について

設定は DB ではなく `<AppData>/B-Listener/settings.json` に保存します。
理由: DB 破損時にも設定（＝保存先フォルダ）を読めなければ復旧できないため。
