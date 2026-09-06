# 03. データフロー

## 1. 全体フロー

```
STEP 1  新しい会議を作成          → meeting(status=draft) を INSERT
STEP 2  任意の事前情報入力        → participant / agenda / context_term を保存（全てスキップ可）
STEP 3  会議開始                  → status=recording、フォルダ作成、録音開始
STEP 4  録音 / リアルタイム文字起こし / リアルタイムAI分析
STEP 5  会議終了                  → 録音停止、WAV確定、status=processing
STEP 6  高精度文字起こし          → 音声ファイル全体を再処理（リアルタイム結果は使わない）
STEP 7  AI最終分析                → 構造化抽出 → 議事録 → まとめ
STEP 8  表示・保存                → status=completed
```

---

## 2. 録音データフロー（最優先経路）

```
 マイク
   │
   ▼
 cpal 入力ストリーム（OSのオーディオコールバックスレッド）
   │  ・ここでは確保・ロック・I/O を一切行わない
   │  ・デバイスのサンプルレート/チャンネル数のまま f32 で受け取る
   ▼
 ダウンミックス(mono) → リサンプル(16kHz)      ※ コールバック内は演算のみ
   │
   ├────────────────────────────────┐
   ▼ (1) 録音経路：絶対に失わない        ▼ (2) リアルタイム経路：捨ててよい
 bounded channel (blocking send)     bounded channel (try_send)
   │                                  │ 満杯なら破棄しカウンタを +1
   ▼                                  ▼
 [Writer スレッド]                   [Realtime STT スレッド]
   │  BufWriter で WAV へ追記
   │  5 秒ごとに:
   │    1. flush()
   │    2. RIFF/data のサイズ欄を実長で上書き
   │    3. file.sync_data()
   ▼
 audio.wav（常に「その時点まで再生可能」な状態）
```

### なぜこれで失われないのか

- **メモリに溜め込まない。** 5秒より古い音声は必ずディスク上にあります。
- **WAV ヘッダを定期更新する。** どのタイミングで落ちても、直前の更新時点までは
  正しいヘッダを持つ再生可能ファイルが残ります。
- **ヘッダが古くても復旧できる。** ヘッダのサイズ欄より実ファイルが大きい場合、
  余剰バイトは有効な PCM です。起動時にヘッダを実サイズへ書き直すだけで全て復元できます。
- **リアルタイム処理は録音をブロックしない。** 経路 (2) は `try_send` で、
  詰まったら捨てます。whisper や Ollama が固まっても録音は継続します。

### 一時停止 / 再開

一時停止中はコールバックからのサンプルを破棄します（ストリーム自体は止めません。
デバイスの再取得で失敗するリスクを避けるため）。`duration_ms` は一時停止分を除いて積算します。

### クラッシュ復旧

```
アプリ起動
  → status IN ('recording','paused') の meeting を検索
  → 見つかった場合: audio.wav を検査
        ヘッダの data サイズ  vs  実ファイルサイズ
          ├ 実ファイルの方が大きい → ヘッダを実サイズに修復
          ├ 一致                   → そのまま
          └ ファイルが無い/ヘッダ壊れ → 生PCMとみなしヘッダを再生成
  → 「中断された会議があります」ダイアログ
        ├ [復旧して処理する] → status=processing → 終了後パイプラインへ
        └ [録音だけ保存]     → status=completed（文字起こし・AIなし）
```

**破棄という選択肢はデフォルトでは提示しません。** 音声は必ず残します。

---

## 3. リアルタイム経路

```
16kHz PCM
  │
  ▼
 リングバッファ（直近 60 秒）
  │
  ▼
 VAD（エネルギー + ゼロ交差率）
  │  ・発話区間の切れ目を探す
  │  ・無音が 600ms 続く、または 20 秒経過でチャンク確定
  ▼
 チャンク（10〜20秒 / 前チャンクと 1 秒オーバーラップ）
  │
  ▼
 whisper (small, greedy, 日本語固定)
  │  ・initial_prompt に事前入力の固有名詞を投入
  ▼
 用語補正（context_term との編集距離マッチ）
  │
  ├──→ transcript_segment(kind='realtime') へ INSERT
  ├──→ Tauri event `transcript:segment` → UI に追記表示
  └──→ Analyzer の入力バッファへ
         │
         │  30〜60 秒ごと（設定可能）
         ▼
      コンテキスト構築（全文は送らない）
         ・直近 3 分の発言
         ・これまでの要約（LLM が更新し続ける 400 字程度）
         ・これまでの決定事項リスト
         ・これまでのネクストアクションリスト
         ・事前情報（議題・参加者・用語）
         ▼
      Ollama（JSON Schema 指定 / format=json / temperature=0.1）
         ▼
      analysis_snapshot へ INSERT
         ▼
      Tauri event `analysis:update` → UI 右ペイン更新
```

リアルタイム分析が失敗した場合、UI にはその旨の小さな表示を出し、**次の周期で再試行**します。
録音・文字起こしには影響しません。

---

## 4. 会議終了後パイプライン

各ステップは `job_run` に状態を記録します。途中で失敗しても**完了済みステップはやり直しません**。

```
 [1] finalize_audio
        録音停止 → WAV ヘッダ確定 → sync → 長さ計測 → duration_ms 確定
        ✔ ここまで完了すれば「音声だけは必ずある」状態が確定する
        │
 [2] transcribe                                    ← リアルタイム結果は使わない
        audio.wav 全体を whisper (medium/large-v3-turbo) で再処理
        ・beam search, best_of=5（精度優先設定）
        ・initial_prompt に事前情報の固有名詞
        ・30秒窓 + タイムスタンプ付きセグメント
        → transcript_segment(kind='final') / transcript.raw.txt
        │
 [3] correct
        context_term / participant によるゆらぎ補正
        → transcript.txt（補正後・タイムスタンプ付き）
        │
 [4] extract   ← 長時間会議対策の中核
        文字起こしを 10 分相当のチャンクに分割（発話境界で切る）
        各チャンクについて:
            LLM → 構造化 JSON（topic / decisions / actions / warnings / pending / important）
            → 即座に meeting_note / action_item へ INSERT     ★消失防止
            → 併せてチャンク要約テキストを保持
        ✔ ここで失敗しても、成功したチャンク分の抽出結果は DB に残る
        │
 [5] merge
        構造化データ: 重複排除・統合（LLM ではなくコードで実施 → 情報が消えない）
                      ・正規化（空白/記号）した完全一致で除去
                      ・類似判定は LLM に「重複か」だけを問い、内容は書き換えさせない
        要約テキスト: 部分要約を結合 → LLM で全体要約を生成
        │
 [6] minutes
        会議情報 + 議題ごとの内容/結論 + 決定事項 + ネクストアクション
        + 注意事項 + 保留 + 次回確認  → minutes.md
        │
 [7] summary
        短く実務的なまとめ（内容/決定事項/ネクストアクション/注意事項/保留/次回確認）
        → summary.md
        │
 [8] persist
        metadata.json 更新 → status=completed → UI へ完了通知
```

### 失敗時の挙動

| 失敗箇所 | 残るもの | UI 表示 |
|---|---|---|
| [2] 文字起こし | 音声 | 「文字起こしに失敗しました。音声は保存されています」＋再実行ボタン |
| [4] 抽出 | 音声 / 文字起こし / 成功済みチャンクの抽出結果 | 「AI分析に失敗しました」＋再実行ボタン |
| [6][7] 生成 | 音声 / 文字起こし / 構造化データ | 構造化データからテンプレートで最低限の議事録を出力し、AI文章生成のみ再実行可能 |

---

## 5. コンテキストサイズの管理

7B クラスのローカルモデルを前提に、1リクエストあたり **入力 6000 トークン以内**を目安にします。

| 用途 | 入力 |
|---|---|
| リアルタイム分析 | 直近3分の発言 + 現在の要約 + 決定事項/アクション一覧 + 事前情報 |
| チャンク抽出 | 該当チャンク全文（約10分 ≒ 3000〜4000字）+ 事前情報 |
| 全体要約 | 各チャンク要約の連結（1チャンク200字 × N）+ 事前情報 |
| 議事録生成 | 統合済み構造化データ + 全体要約 + 事前情報（**文字起こし全文は渡さない**） |

チャンク数が多く全体要約が長くなる場合は、要約を再度チャンク化して2段階で統合します。

---

## 6. Tauri IPC 一覧（抜粋）

### commands（UI → Rust）

| command | 説明 |
|---|---|
| `create_meeting` | 会議を作成（draft） |
| `update_meeting_context` | 事前情報を保存 |
| `list_meetings` | 過去会議一覧 |
| `get_meeting_detail` | 会議詳細（情報・議事録・まとめ・音声パス） |
| `delete_meeting` | 会議削除 |
| `list_input_devices` | マイク一覧 |
| `start_recording` / `pause_recording` / `resume_recording` / `stop_recording` | 録音制御 |
| `get_recording_state` | 経過時間・レベル・状態 |
| `run_pipeline` / `retry_pipeline_step` | 終了後処理 |
| `get_settings` / `update_settings` | 設定 |
| `check_components` | Whisper モデル / Ollama の導入状況 |
| `download_whisper_model` / `list_ollama_models` | セットアップ支援 |
| `open_meeting_folder` / `export_meeting` | 出力 |
| `get_recoverable_meetings` / `recover_meeting` | クラッシュ復旧 |

### events（Rust → UI）

| event | 説明 |
|---|---|
| `recording:tick` | 経過時間・入力レベル・書き込み済みバイト数 |
| `recording:error` | 録音エラー（デバイス切断など） |
| `transcript:segment` | リアルタイム文字起こしの1セグメント |
| `analysis:update` | リアルタイムAI分析の最新結果 |
| `pipeline:progress` | 終了後処理の進捗（ステップ名・%） |
| `pipeline:done` / `pipeline:failed` | 完了・失敗 |
| `model:download-progress` | モデルDL進捗 |
