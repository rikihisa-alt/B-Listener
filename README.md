# B-Listener

会議を録音し、終了後に **音声 / 議事録 / AIまとめ** を確実に出力するデスクトップアプリ（Windows / macOS）。

すべての処理はローカルで完結します。会議音声・文字起こし・個人情報は一切外部へ送信しません。運用時に有料APIは使用しません。

---

## これは何か

```
アプリ起動 → 「新しい会議」 → （任意で事前情報） → 「会議開始」
   → 会議（録音 + リアルタイム文字起こし + リアルタイムAI分析）
   → 「会議終了」 → 自動処理 → 完成
                                ├─ audio.wav   音声
                                ├─ minutes.md  議事録
                                └─ summary.md  AIまとめ
```

ユーザーが Whisper や Ollama といった技術を意識する必要はありません。

---

## 設計方針（優先順位）

| # | 方針 | 設計への反映 |
|---|------|------------|
| 1 | **録音データを絶対に失わない** | ネイティブ録音スレッドが WAV へ逐次追記。メモリ保持なし。5秒ごとに flush + ヘッダ更新。クラッシュ後は起動時に自動復旧 |
| 2 | 最終文字起こし精度を重視 | リアルタイム結果は最終議事録に使わない。終了後に音声全体を高精度モデルで再処理 |
| 3 | AIまとめの事実性を重視 | 構造化出力（JSON Schema）+ 推測禁止プロンプト + 出力後の事実性検証 |
| 4 | UIはシンプル | 業務用UI。装飾なし。「会議開始」「会議終了」を明確に |
| 5 | 有料外部API非依存 | whisper.cpp（アプリ内蔵）+ Ollama（ローカル） |
| 6 | 過剰機能を作らない | MVP対象外機能は [docs/04-roadmap.md](docs/04-roadmap.md) に明記 |
| 7 | MVPを確実に完成させる | Phase 1→9 の段階実装。Phase 5 時点で MVP 完成条件を満たす |

**独立性の原則**: リアルタイム処理・AI処理が全て失敗しても、**音声ファイルは必ず残る**。
文字起こしが失敗しても音声は残る。AI分析が失敗しても音声と文字起こしは残る。

---

## 設計ドキュメント

| ドキュメント | 内容 |
|---|---|
| [docs/01-architecture.md](docs/01-architecture.md) | 技術構成 / 構成変更の理由 / アーキテクチャ / ディレクトリ構成 |
| [docs/02-database.md](docs/02-database.md) | DB設計（SQLite スキーマ） |
| [docs/03-dataflow.md](docs/03-dataflow.md) | データフロー（録音 / リアルタイム / 終了後パイプライン / 復旧） |
| [docs/04-roadmap.md](docs/04-roadmap.md) | 各Phaseの実装内容と完了条件 |
| [docs/05-risks.md](docs/05-risks.md) | リスクと対策 / Windows・macOS の注意点 |
| [docs/06-prompts.md](docs/06-prompts.md) | AIプロンプト設計と構造化出力スキーマ |

---

## 開発環境セットアップ

### 必要なもの

| ツール | 用途 | 必須 |
|---|---|---|
| Node.js 20+ | フロントエンド | 開発時必須 |
| Rust (stable) | Tauri バックエンド | 開発時必須 |
| CMake 3.20+ | whisper.cpp のビルド | 開発時必須 |
| Xcode Command Line Tools (macOS) / Visual Studio Build Tools (Windows) | リンカ | 開発時必須 |
| Ollama | ローカルLLM | 実行時（AI機能を使う場合） |

### macOS

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
xcode-select --install
python3 -m pip install --user cmake
```

Ollama は https://ollama.com/download からインストール後:

```bash
ollama pull qwen2.5:7b-instruct
```

### 起動

```bash
npm install
npm run tauri dev
```

---

## 保存されるもの

会議ごとに1フォルダ:

```
Meetings/
  2026-09-05_運営会議/
    audio.wav          録音（16kHz / mono / 16bit PCM）
    transcript.txt     最終文字起こし（タイムスタンプ付き）
    transcript.raw.txt 補正前の文字起こし
    minutes.md         議事録
    summary.md         AIまとめ
    metadata.json      会議情報・構造化データ（決定事項/ネクストアクション等）
```

DB (`app.db`) は検索・一覧用のインデックスであり、**唯一の正はフォルダ内のファイル**です。
DBが壊れてもフォルダから再構築できます。

---

## リリース（配布用ビルド）

タグを push すると GitHub Actions が macOS / Windows のインストーラを作成し、
**下書きリリース**に添付します。内容を確認してから公開してください。

```bash
git tag v0.1.0
git push origin v0.1.0
```

| ワークフロー | 契機 | 内容 |
|---|---|---|
| `.github/workflows/ci.yml` | push / PR | 型チェック・ビルド・`cargo fmt`・`clippy`・テスト |
| `.github/workflows/release.yml` | `v*` タグ | macOS (Apple Silicon / Intel)・Windows のインストーラ作成 |

### 署名について

現時点では**コード署名を行っていません**。そのため:

- **macOS**: 初回起動時に Gatekeeper の警告が出ます。右クリック →「開く」で回避できます。
  正式配布するには Apple Developer Program（年間 99 USD）による署名と公証が必要です。
- **Windows**: SmartScreen の警告が出ます。回避するには証明書による署名が必要です。

社内配布であれば警告を回避する手順を案内する形で運用できます。
外部配布を行う段階になったら、署名鍵を GitHub Secrets に登録してワークフローを拡張してください。

---

## 実装状況

| Phase | 内容 | 状態 |
|---|---|---|
| 1 | アプリ基盤（ホーム / 新規会議 / 会議一覧 / SQLite / 設定 / ログ） | ✅ 完了 |
| 2 | 録音（開始・一時停止・再開・終了・逐次保存・クラッシュ復旧） | ✅ 完了 |
| 3 | 終了後の高精度文字起こし（whisper.cpp 内蔵 / モデル自動DL） | ✅ 完了 |
| 4 | Ollama 連携 / AIまとめ | 未着手 |
| 5 | 議事録生成（★ここで MVP 完成） | 未着手 |
| 6 | 事前入力機能 | ✅ UI・保存・whisper への反映まで完了 |
| 7 | リアルタイム文字起こし | 未着手 |
| 8 | リアルタイムAI分析 | 未着手 |
| 9 | 外部出力（docx / m4a）・UI改善 | 未着手 |

---

## 開発コマンド

```bash
npm install                 # 依存関係の取得（初回のみ）
npm run tauri dev           # 開発モードで起動（ホットリロードあり）
npm run typecheck           # TypeScript の型チェック
npm run build               # フロントエンドのビルド
```

Rust 側:

```bash
cd src-tauri && cargo test
```

macOS でマイクを使うには、Info.plist を含む .app として起動する必要があります。

```bash
npx tauri build --debug --bundles app
```

生成物: `src-tauri/target/debug/bundle/macos/B-Listener.app`

### 文字起こしの統合テスト

音声認識モデルと検証用音声が必要です。検証用音声は macOS の `say` で作れます。

```bash
say -v Kyoko -o /tmp/bl_speech.aiff "来月のシフトについてですが、新規利用者の対応を検討します。吉田さんが確認してください。期限は火曜日です。訪問介護と訪問看護の調整も必要です。"
afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/bl_speech.aiff /tmp/bl_speech.wav
```

```bash
cd src-tauri && cargo test --test stt_integration -- --ignored --nocapture
```

```bash
cd src-tauri && cargo test --test pipeline_integration -- --ignored --nocapture --test-threads=1
```

`pipeline_integration` は「録音済み音声 → 文字起こし → 保存」の全体と、
「モデルが無い場合でも音声が失われないこと」を検証します。

### 実機マイクを使った録音テスト

`#[ignore]` が付いているため、明示的に指定したときだけ実行されます。

```bash
cd src-tauri && cargo test --test recording_integration -- --ignored --nocapture --test-threads=1
```

検証内容: 録音開始 → 一時停止 → 再開 → 終了、一時停止分が録音長に含まれないこと、
WAV が再生可能な状態で確定すること、二重録音が拒否されること。

---

## Phase 2 の受け入れ確認

| 確認項目 | 方法 | 結果 |
|---|---|---|
| 開始 → 終了 → 再生できる | 統合テスト `records_pauses_resumes_and_finalizes` | ✅ |
| 一時停止 → 再開 で停止分が詰められる | 同上（4秒録音 = 2秒 + 2秒、停止2秒は除外） | ✅ |
| 強制終了後にファイルを復旧できる | 単体テスト `repairs_outdated_header_without_losing_audio` | ✅ |
| ヘッダが壊れても音声を救出できる | 単体テスト `rescues_broken_header_to_new_file` | ✅ |
| 二重録音を拒否する | 統合テスト `rejects_second_concurrent_recording` | ✅ |
| 長時間録音でメモリが増え続けない | 設計上メモリ保持なし（5秒ごとにディスクへ確定） | 要実機確認 |
| マイクを抜いてもそこまでの音声が残る | `recording:error` イベント + finalize 経路を実装済み | 要実機確認 |

## Phase 3 の受け入れ確認

| 確認項目 | 方法 | 結果 |
|---|---|---|
| 音声から日本語の文字起こしができる | 統合テスト `transcribes_japanese_meeting_audio` | ✅ |
| 事前情報の固有名詞が文字起こしに反映される | 同上（「吉田」「訪問介護」「訪問看護」を確認） | ✅ |
| 会議終了後に自動で処理が走る | アプリ実機で確認（音声確定 → 文字起こし → 保存） | ✅ |
| 長時間音声を分割して処理する | 10分チャンク + 無音位置での分割を実装、単体テスト `splits_at_the_quietest_point` | ✅ |
| **文字起こしが失敗しても音声は残る** | 統合テスト `keeps_audio_when_transcription_fails` | ✅ |
| モデル未導入が利用者に伝わる | 設定画面に「未導入」表示 + ダウンロード導線 | ✅ |
