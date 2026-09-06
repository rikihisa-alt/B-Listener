//! ブラウザ版の通知送り先（SSE へ流すためのブロードキャスト）。

use tokio::sync::broadcast;

use crate::events::{AppEvent, EventSink};

/// SSE で配信する 1 件の通知。
#[derive(Debug, Clone)]
pub struct SseMessage {
    pub name: &'static str,
    pub data: String,
}

/// 接続中の全ブラウザへ同じ通知を配る。
///
/// 受信側がいない・遅れている場合でもコア処理を止めないよう、
/// 送信は常にノンブロッキングで、あふれたぶんは捨てる。
pub struct BroadcastSink {
    sender: broadcast::Sender<SseMessage>,
}

impl BroadcastSink {
    /// `capacity` は 1 クライアントあたりの取りこぼし許容量。
    /// 録音中の tick が 0.5 秒間隔なので、数十件あれば十分。
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SseMessage> {
        self.sender.subscribe()
    }
}

impl EventSink for BroadcastSink {
    fn emit(&self, event: AppEvent) {
        let message = SseMessage {
            name: event.name(),
            data: event.payload().to_string(),
        };
        // receiver が 0 件のときは Err になるが、それは異常ではない
        // （まだブラウザが繋がっていないだけ）。
        let _ = self.sender.send(message);
    }
}
