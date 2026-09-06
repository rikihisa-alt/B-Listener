# 06. AIプロンプト設計と構造化出力

## 方針

- プロンプトは `src-tauri/src/llm/prompts/*.md` に置き、`include_str!` で読み込む。コード中に文字列で散らさない
- 変数は `{{name}}` 形式のプレースホルダで埋め込む
- 出力は必ず JSON。Ollama の `format` に JSON Schema を渡し、`temperature=0.1` で実行する
- パースに失敗した場合は 1 回だけ「JSONのみを返せ」と再指示する。2 回失敗したらそのステップを失敗として記録する（握り潰さない）

---

## 共通システム指示（全プロンプトの先頭に必ず入る）

`prompts/system_common.md`

```
あなたは会議議事録作成AIです。
与えられた会議文字起こしおよび事前情報のみを根拠として分析してください。

以下を厳守してください。

- 発言されていない事実を作らない
- 推測を事実として扱わない
- 決定事項と検討中事項を区別する
- 担当者不明の場合は担当者未定
- 期限不明の場合は期限未定
- 同じ内容を重複させない
- 会議に関係のない雑談は必要に応じて省略する
- 数字、日付、人名を可能な限り保持する
- 事前情報は補助情報であり、実際の会議発言を優先する
- 事前情報だけを根拠に「会議で決定した」と判断しない

出力は指定されたJSON形式のみとし、説明文やコードブロック記法を含めないでください。
該当する情報が存在しない場合は、無理に埋めず空の配列または空文字列を返してください。
```

---

## 事前情報ブロック（共通）

`prompts/_context_block.md`

```
## 事前情報（補助。会議で実際に発言された内容が優先）

会議名: {{title}}
日時: {{scheduled_at}}
参加者: {{participants}}
主な議題:
{{agendas}}
今回決めたいこと: {{goal}}
前回からの持ち越し事項: {{carryover}}
補足情報: {{notes}}

固有名詞・専門用語（文字起こしの誤変換を補正する参考にしてください）:
{{terms}}

注意: これらは会議前に入力された参考情報です。
ここに書かれているだけの事柄を「会議で決定した」「会議で発言された」として扱わないでください。
```

---

## 1. リアルタイム分析 `prompts/realtime_analysis.md`

**入力**: 直近3分の発言 / これまでの要約 / これまでの決定事項 / これまでのネクストアクション / 事前情報

```
{{system_common}}

{{context_block}}

## これまでの会議の要約
{{rolling_summary}}

## これまでに確認された決定事項
{{known_decisions}}

## これまでに確認されたネクストアクション
{{known_actions}}

## 直近の発言（リアルタイム文字起こしのため誤変換を含みます）
{{recent_transcript}}

## 指示
直近の発言を踏まえ、会議の現在の状態を更新してください。
- currentTopic: 今まさに話されている議題。判断できない場合は空文字列
- decisions: 明確に決まったこと**のみ**。「検討する」「持ち帰る」は decisions ではなく pendingItems
- actions: 誰かが行うと発言された作業。担当者が発言されていなければ person は空文字列、
  期限が発言されていなければ deadline は空文字列
- warnings: 注意すべき点・リスク・共有事項として発言されたもの
- pendingItems: まだ決まっていない・保留になったもの
- importantPoints: 数値・日付・条件など、後から参照する必要がある事実

既に「これまでに確認された」に含まれる内容は繰り返さず、新規・更新分のみを返してください。
rollingSummary は会議全体の要約を400字以内で更新したものを返してください。
```

**出力スキーマ**

```json
{
  "type": "object",
  "properties": {
    "currentTopic":   { "type": "string" },
    "decisions":      { "type": "array", "items": { "type": "string" } },
    "actions": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "person":   { "type": "string" },
          "action":   { "type": "string" },
          "deadline": { "type": "string" }
        },
        "required": ["person", "action", "deadline"]
      }
    },
    "warnings":        { "type": "array", "items": { "type": "string" } },
    "pendingItems":    { "type": "array", "items": { "type": "string" } },
    "importantPoints": { "type": "array", "items": { "type": "string" } },
    "rollingSummary":  { "type": "string" }
  },
  "required": ["currentTopic","decisions","actions","warnings","pendingItems","importantPoints","rollingSummary"]
}
```

---

## 2. チャンク抽出 `prompts/chunk_extract.md`

長時間会議を10分程度に分割し、各チャンクから構造化情報を取り出します。
**このステップの出力は即座に DB へ保存され、以降の処理で失われません。**

```
{{system_common}}

{{context_block}}

## 会議の文字起こし（第{{chunk_index}}/{{chunk_total}}部・{{time_range}}）
{{chunk_text}}

## 指示
この部分について、以下を抽出してください。
- topics: この部分で話された議題。議題ごとに、話された内容(content)と結論(conclusion)を書く。
          結論が出ていない場合 conclusion は空文字列
- decisions: この部分で明確に決定されたこと
- actions: 実行すると発言された作業。person / deadline は発言されていなければ空文字列。
           deadlineRaw には発言された表現をそのまま入れる（例: 「火曜まで」「来週中」）
- warnings: 注意点・リスク・共有事項
- pendingItems: 保留・未決事項
- nextChecks: 次回確認すべきと発言されたもの
- importantPoints: 保持すべき数値・日付・固有名詞を含む事実
- summary: この部分の要約（200字以内）

前後の部分の内容を推測して補わないでください。この部分に含まれる情報だけを扱ってください。
```

**出力スキーマ**（`ChunkExtraction`）

```json
{
  "type": "object",
  "properties": {
    "topics": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "title":      { "type": "string" },
          "content":    { "type": "string" },
          "conclusion": { "type": "string" }
        },
        "required": ["title","content","conclusion"]
      }
    },
    "decisions": { "type": "array", "items": { "type": "string" } },
    "actions": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "person":       { "type": "string" },
          "action":       { "type": "string" },
          "deadline":     { "type": "string" },
          "deadlineRaw":  { "type": "string" }
        },
        "required": ["person","action","deadline","deadlineRaw"]
      }
    },
    "warnings":        { "type": "array", "items": { "type": "string" } },
    "pendingItems":    { "type": "array", "items": { "type": "string" } },
    "nextChecks":      { "type": "array", "items": { "type": "string" } },
    "importantPoints": { "type": "array", "items": { "type": "string" } },
    "summary":         { "type": "string" }
  },
  "required": ["topics","decisions","actions","warnings","pendingItems","nextChecks","importantPoints","summary"]
}
```

---

## 3. 重複判定 `prompts/dedupe.md`

構造化データの統合では、**LLM に内容を書き換えさせません。**
「どれとどれが同一か」だけを問い、実際の削除・保持はコードが行います。

```
{{system_common}}

## 項目一覧
{{numbered_items}}

## 指示
上記のうち、同じ事柄を指している項目の番号をグループ化してください。
表現が違っても内容が同一なら同じグループにします。
少しでも異なる情報を含む場合は別グループにしてください。
どのグループにも属さない項目は、単独のグループとして返してください。

出力形式: {"groups": [[1,4],[2],[3,5,6]]}
```

---

## 4. 全体要約 `prompts/overall_summary.md`

```
{{system_common}}

{{context_block}}

## 各部分の要約（時系列）
{{partial_summaries}}

## 指示
会議全体の流れを800字以内でまとめてください。
各部分の要約に書かれていないことを補わないでください。
出力形式: {"summary": "..."}
```

---

## 5. 議事録生成 `prompts/minutes.md`

文字起こし全文は渡しません。統合済みの構造化データと全体要約のみを渡します。

```
{{system_common}}

{{context_block}}

## 会議全体の要約
{{overall_summary}}

## 抽出済みの議題
{{topics}}

## 抽出済みの決定事項
{{decisions}}

## 抽出済みのネクストアクション
{{actions}}

## 抽出済みの注意事項
{{warnings}}

## 抽出済みの保留・未決事項
{{pending_items}}

## 抽出済みの次回確認事項
{{next_checks}}

## 指示
上記の情報のみを用いて議事録の本文を作成してください。
- agendaSections: 議題ごとに title / content / conclusion を整理する。
  content は抽出済みの内容を読める日本語に整えたもの。新しい事実を足さない。
  結論が出ていない議題の conclusion は「結論なし（継続審議）」とする
- 与えられていない情報を追加しない
- 与えられた情報を落とさない

出力形式:
{"agendaSections":[{"title":"","content":"","conclusion":""}]}
```

**Markdown への組み立てはコード側（`storage/markdown.rs`）で行います。**
会議情報・決定事項・ネクストアクション・注意事項・保留・次回確認は
DB の構造化データから直接出力するため、LLM の生成過程で欠落することがありません。

`minutes.md` の構成:

```markdown
# {会議名}

## 会議情報
- 日時 / 開始時刻 / 終了時刻 / 会議時間 / 参加者

## 会議内容
### 議題1: {title}
**内容**
**結論**

## 決定事項
## ネクストアクション   （担当者 / 内容 / 期限 の表）
## 注意事項
## 保留・未決事項
## 次回確認事項
```

---

## 6. AIまとめ `prompts/summary.md`

```
{{system_common}}

{{context_block}}

## 会議全体の要約
{{overall_summary}}

## 抽出済みの構造化データ
{{structured_data}}

## 指示
実務で使える短いまとめを作成してください。
- content: 今回何を話したかを簡潔に（400字以内）
- decisions: 今回決まったこと
- actions: 誰が / 何を / いつまでに。発言上確認できない場合は person を「担当者未定」、
  deadline を「期限未定」とする
- warnings: 重要な注意点、リスク、共有事項
- pendingItems: まだ決まっていないこと
- nextChecks: 次回確認すべきもの

与えられていない情報を補完・創作しないでください。
```

**出力スキーマ**（`MeetingSummary`）: `content` / `decisions` / `actions` / `warnings` / `pendingItems` / `nextChecks`

---

## 7. 出力後の事実性検証（`llm/verify.rs`）

LLM の出力をそのまま信用せず、機械的に検証します。

| 検証 | 内容 | 不合格時 |
|---|---|---|
| 人名 | `person` が `participant` / `context_term(person)` / 文字起こし本文に出現するか | 破棄せず `needs_review` を立て、UI で「要確認」と表示 |
| 数値・日付 | 決定事項・重要ポイント中の数値/日付が文字起こしに出現するか | 同上 |
| 空値の正規化 | `person` が「不明」「なし」等なら空文字に統一 | 表示時に「担当者未定」 |
| 重複 | 正規化後の完全一致を除去 | 除去 |

**破棄ではなくフラグ付けにする理由**: 誤検知で本物の決定事項を消すほうが、
要確認マークが付くより実務上の損害が大きいためです。
