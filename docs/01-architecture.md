# 01. 技術構成とアーキテクチャ

## 1. 技術構成

| 層 | 採用技術 | 理由 |
|---|---|---|
| デスクトップシェル | **Tauri 2** | ネイティブスレッドで録音を制御でき、最優先要件「録音を失わない」を満たせる。配布サイズが小さくメモリ消費も少ない（3時間の会議中に常駐する） |
| フロントエンド | **React 18 + TypeScript + Vite** | 型安全。UIとロジックの分離がしやすい |
| バックエンド | **Rust** | 音声のリアルタイムスレッド処理、長時間の安定動作、whisper.cpp との直接結合 |
| DB | **SQLite (rusqlite, bundled)** | 外部依存ゼロ。`bundled` feature で SQLite 本体を静的リンクするためユーザー環境に SQLite 不要 |
| 音声入力 | **cpal** | Windows(WASAPI) / macOS(CoreAudio) を単一APIで扱える |
| 音声書き出し | 自前 `WavSink`（hound 相当を内製） | 「途中で落ちても復旧できる」ためにヘッダ更新タイミングを自分で制御する必要がある |
| 音声認識 | **whisper-rs**（whisper.cpp の Rust バインディング） | アプリに内蔵。ユーザーは何もインストール不要。macOS では Metal、Windows では CPU/Vulkan |
| ローカルLLM | **Ollama**（HTTP / localhost:11434） | 導入が容易。`LlmProvider` trait で抽象化し他エンジンへ差し替え可能 |
| 初期LLMモデル | **qwen2.5:7b-instruct** | 日本語性能とローカル実行性能のバランスが良く、JSON構造化出力が安定 |

---

## 2. 想定構成からの変更点と理由

### 2.1 音声形式: m4a → **16kHz / mono / 16bit PCM WAV**（マスタ）

最優先要件「録音データを絶対に失わない」に直結する変更です。

- **WAV は末尾が壊れても復旧できる。** ヘッダ以降は生PCMが並ぶだけなので、アプリが強制終了しても「ファイル末尾までのバイト列」がそのまま有効な音声です。復旧はヘッダのサイズ欄を実ファイルサイズに書き直すだけで済みます。
  m4a / Opus はコンテナのインデックス（moov atom 等）がファイル末尾に書かれるため、書き終わる前に落ちると**全損**します。これは最優先要件と真っ向から衝突します。
- **whisper.cpp の入力要件が 16kHz mono。** 録音時点で合わせておけば変換工程が不要になり、失敗しうる処理段が1つ減ります。
- **サイズは許容範囲。** 16kHz/mono/16bit = 32KB/秒 → 3時間で約 **346MB**。業務PCのディスクで問題になる量ではありません。

> m4a への圧縮は「会議終了後の任意の後処理」として Phase 9 で実装します（macOS は OS 標準の `afconvert`）。
> 圧縮後の再生検証に成功するまで WAV は削除しません。設定で「WAVも残す」を選べます。

### 2.2 音声認識: whisper.cpp CLI サイドカー → **whisper-rs（アプリ内蔵）**

「ユーザーが技術を意識しない」要件のため。外部バイナリの同梱・パス解決・プロセス起動失敗といった失敗要因を排除できます。モデルファイルのみ初回に自動ダウンロードします。

### 2.3 LLM 呼び出しを `LlmProvider` trait で抽象化

「将来モデルを変更可能な設計にする」ルールに対応。Ollama 実装に加え、OpenAI互換ローカルサーバ（LM Studio / llama.cpp server）を設定から選べる形にします。**外部クラウドAPIの実装は含めません。**

### 2.4 `Decision` / 警告 / 保留などを `meeting_note` に統合

指定された `Decision` テーブルに加え、警告・保留・次回確認も同じ「会議から抽出された1行のテキスト」という同型のデータです。種別カラムで1テーブルに統合し、コード重複を避けます（詳細は [02-database.md](02-database.md)）。

---

## 3. アーキテクチャ

### 3.1 レイヤ構成

```
┌──────────────────────────────────────────────┐
│ UI 層 (React / TypeScript)                    │
│   画面描画と入力のみ。ビジネスロジックを持たない  │
└───────────────┬──────────────────────────────┘
                │ Tauri IPC (command: 要求 / event: 通知)
                │ 型は src/types/ipc.ts ⇔ Rust serde で1:1対応
┌───────────────▼──────────────────────────────┐
│ commands/ (薄いアダプタ層)                     │
│   引数検証 → core 呼び出し → DTO 変換のみ       │
└───────────────┬──────────────────────────────┘
                │
┌───────────────▼──────────────────────────────┐
│ Core (Rust)                                   │
│                                               │
│  audio/     ─ 録音・WAV書込・復旧・VAD         │  ← 互いに独立
│  stt/       ─ 文字起こし (Transcriber trait)   │  ← 互いに独立
│  llm/       ─ AI分析   (LlmProvider trait)     │  ← 互いに独立
│  pipeline/  ─ 上記3つを順に呼ぶ状態機械        │
│  db/        ─ SQLite リポジトリ                │
│  storage/   ─ 会議フォルダ・ファイル出力        │
└──────────────────────────────────────────────┘
```

**音声処理とAI処理は分離されています。** `audio` は `stt` / `llm` を知りません。`stt` は `llm` を知りません。
結合は `pipeline` のみが行うため、AI が落ちても録音・保存は影響を受けません。

### 3.2 スレッドモデル（会議中）

```
 [cpal コールバックスレッド] ← OSのリアルタイムスレッド。ここでは何もブロックしない
        │ f32 サンプル
        ├──(1) SPSC channel (bounded, blocking send) ──→ [Writer スレッド]
        │                                                   └─ WAV へ追記 / 5秒ごとに flush + ヘッダ更新
        │
        └──(2) try_send (bounded, ノンブロッキング) ─────→ [Realtime STT スレッド]
                 ※ 満杯なら破棄。録音は絶対に止めない          └─ VAD → 10〜20秒チャンク → whisper(small)
                                                                    └→ transcript イベント → UI
                                                                    └→ [Analyzer スレッド] 30〜60秒ごと → LLM → 分析イベント → UI
```

経路 (1) は録音の生命線なので優先度が最も高く、経路 (2) は**いつでも捨ててよい**設計です。
リアルタイム処理が詰まっても録音は 1 サンプルも失われません。

---

## 4. ディレクトリ構成

```
B-Listener/
├── README.md
├── docs/                          設計ドキュメント
├── package.json
├── vite.config.ts
├── tsconfig.json
├── index.html
│
├── src/                           ─── フロントエンド (React/TS)
│   ├── main.tsx
│   ├── App.tsx
│   ├── routes.tsx
│   ├── types/
│   │   └── ipc.ts                 Rust と対応する型定義（IPC の正）
│   ├── lib/
│   │   ├── ipc.ts                 invoke の型付きラッパ
│   │   ├── events.ts              Tauri イベント購読
│   │   ├── format.ts              日時・時間の整形
│   │   └── errors.ts              エラー表示への変換
│   ├── store/                     zustand（UI状態のみ）
│   ├── components/
│   │   ├── ui/                    Button / Card / Dialog / Field ...
│   │   └── layout/
│   ├── features/
│   │   ├── home/                  ホーム・過去会議一覧
│   │   ├── meeting-new/           事前入力（スキップ可）
│   │   ├── meeting-live/          会議中
│   │   ├── meeting-result/        終了後処理・結果表示
│   │   ├── meeting-detail/        過去会議の詳細
│   │   └── settings/              設定・コンポーネント検出
│   └── styles/
│
└── src-tauri/                     ─── バックエンド (Rust)
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── capabilities/
    ├── icons/
    └── src/
        ├── main.rs
        ├── lib.rs                 AppState 構築・command 登録
        ├── error.rs               AppError（thiserror）／握り潰さない
        ├── logging.rs             tracing → ローテーションログ
        ├── settings.rs            AppSettings の読み書き
        ├── paths.rs               保存先・フォルダ名サニタイズ
        │
        ├── db/
        │   ├── mod.rs
        │   ├── migrations.rs      スキーマ・マイグレーション
        │   ├── models.rs
        │   └── repo/              meeting / participant / term / segment / note / action ...
        │
        ├── audio/
        │   ├── mod.rs
        │   ├── devices.rs         入力デバイス列挙・既定デバイス
        │   ├── recorder.rs        録音セッション（開始/一時停止/再開/終了）
        │   ├── wav_sink.rs        逐次書き込み + flush + ヘッダ更新
        │   ├── resample.rs        デバイスレート → 16kHz mono
        │   ├── ring.rs            リアルタイム用リングバッファ
        │   ├── vad.rs             エネルギーベース発話区間検出
        │   └── recovery.rs        クラッシュ後の WAV 修復
        │
        ├── stt/
        │   ├── mod.rs             Transcriber trait
        │   ├── whisper.rs         whisper-rs 実装
        │   ├── models.rs          モデル一覧・DL・ハッシュ検証
        │   ├── realtime.rs        リアルタイム文字起こしワーカー
        │   └── correction.rs      事前情報による用語補正
        │
        ├── llm/
        │   ├── mod.rs             LlmProvider trait
        │   ├── ollama.rs
        │   ├── openai_compat.rs
        │   ├── schema.rs          構造化出力の型 + JSON Schema
        │   ├── prompts/           *.md（プロンプトはここに集約）
        │   ├── chunking.rs        長時間会議の階層要約
        │   ├── realtime_analysis.rs
        │   ├── minutes.rs
        │   ├── summary.rs
        │   └── verify.rs          出力の事実性検証
        │
        ├── pipeline/
        │   ├── mod.rs             終了後処理オーケストレータ
        │   └── steps.rs           各ステップ（再開可能）
        │
        ├── storage/
        │   ├── mod.rs             会議フォルダ構成
        │   ├── metadata.rs        metadata.json
        │   ├── markdown.rs        minutes.md / summary.md
        │   └── docx.rs            docx 出力（Phase 9）
        │
        └── commands/
            ├── mod.rs
            ├── meeting.rs
            ├── recording.rs
            ├── pipeline.rs
            ├── settings.rs
            └── system.rs          コンポーネント検出・フォルダを開く
```

---

## 5. 開発ルール（コードに対する規約）

- `any` を使わない。IPC の型は `src/types/ipc.ts` に集約し、Rust の serde 定義と 1:1 で対応させる
- UI コンポーネントは `invoke` を直接呼ばない。`lib/ipc.ts` 経由のみ
- Rust 側で `unwrap()` / `expect()` を通常フローに使わない。`AppError` で返す
- エラーは必ずログに残す（`tracing`）。握り潰し禁止
- プロンプトは `llm/prompts/*.md` に置き `include_str!` で読む。コード中に文字列で散らさない
- パスは `PathBuf`。保存先・モデル名・間隔などのハードコード禁止（`settings.rs` に集約）
- 外部通信は「モデルのダウンロード」と「localhost の Ollama」のみ。会議データの外部送信コードは存在させない
