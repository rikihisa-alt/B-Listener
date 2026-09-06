//! ブラウザ版の HTTP API。
//!
//! # 設計
//! デスクトップ版の Tauri コマンドと **まったく同じコマンド名・引数** を
//! `POST /api/<command>` で受ける。こうすることでフロントエンドは
//! 「invoke するか fetch するか」だけを差し替えれば動く。
//!
//! # 置き場所についての注意
//! このサーバは**社内のPC/サーバで動かす前提**である。
//! 会議の音声・文字起こし・議事録を外部のクラウドへ出さないという
//! 最優先方針を守るため、インターネット上に公開してはならない。

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::services::{ServeDir, ServeFile};

use crate::db::models::MeetingContextInput;
use crate::error::{AppError, AppResult};
use crate::server::service::{self, MeetingDocument, RecordingSource};
use crate::server::sink::BroadcastSink;
use crate::settings::AppSettings;
use crate::state::AppState;

/// SSE のキープアライブ間隔。プロキシに切られないよう定期的に送る。
const SSE_KEEPALIVE: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct ServerContext {
    pub state: Arc<AppState>,
    pub sink: Arc<BroadcastSink>,
    /// フロントエンドの静的ファイル（dist）の場所。
    pub web_root: PathBuf,
}

/// ルータを組み立てる。
pub fn router(ctx: ServerContext) -> Router {
    let web_root = ctx.web_root.clone();
    let index = web_root.join("index.html");

    // SPA なので、API 以外の未知のパスは index.html に流す
    let static_files = ServeDir::new(&web_root).fallback(ServeFile::new(index));

    Router::new()
        .route("/api/health", get(health))
        .route("/api/events", get(events))
        .route("/api/command/{command}", post(dispatch))
        .route("/api/recording/chunk", post(upload_chunk))
        .route("/api/audio/{meeting_id}", get(serve_audio))
        .route(
            "/api/download/{meeting_id}/{document}",
            get(download_document),
        )
        .fallback_service(static_files)
        .with_state(ctx)
}

pub async fn serve(ctx: ServerContext, addr: SocketAddr) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let actual = listener.local_addr()?;
    tracing::info!(address = %actual, "ブラウザ版サーバを起動しました");
    println!();
    println!("  B-Listener（ブラウザ版）が起動しました");
    println!();
    println!("    このPC        : http://localhost:{}", actual.port());
    for ip in local_ipv4_addresses() {
        println!("    社内LANから   : http://{}:{}", ip, actual.port());
    }
    println!();
    println!("  会議の音声・議事録はこのPC内にのみ保存されます。");
    println!("  終了するには Ctrl+C を押してください。");
    println!();

    axum::serve(listener, router(ctx))
        .with_graceful_shutdown(shutdown_signal())
        .await
}

async fn shutdown_signal() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::error!(error = %e, "終了シグナルを待機できませんでした");
    }
    tracing::info!("終了要求を受け取りました。録音中の場合はファイルを確定します。");
}

/// LAN 内の他PCから接続するためのアドレス候補を表示用に集める。
fn local_ipv4_addresses() -> Vec<String> {
    // 依存を増やさないため、OS のコマンドは使わず UDP ソケットの
    // 「接続先を決めたときに選ばれるローカルアドレス」を利用する。
    use std::net::UdpSocket;
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
        return Vec::new();
    };
    // 実際には送信しない。経路選択のためだけに connect する。
    if socket.connect("192.168.0.1:80").is_err() && socket.connect("10.0.0.1:80").is_err() {
        return Vec::new();
    }
    match socket.local_addr() {
        Ok(addr) if !addr.ip().is_unspecified() => vec![addr.ip().to_string()],
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------- 共通

/// `AppError` をそのまま HTTP レスポンスへ変換する。
/// フロントエンドは Tauri 版と同じ `{code, message}` を受け取る。
struct ApiError(AppError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Invalid(_) => StatusCode::BAD_REQUEST,
            AppError::MissingComponent(_) => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        tracing::warn!(code = self.0.code(), error = %self.0, "APIエラー");
        (
            status,
            Json(json!({ "code": self.0.code(), "message": self.0.to_string() })),
        )
            .into_response()
    }
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        ApiError(e)
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }))
}

// ---------------------------------------------------------------- SSE

async fn events(
    State(ctx): State<ServerContext>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let stream = BroadcastStream::new(ctx.sink.subscribe()).filter_map(|msg| match msg {
        Ok(m) => Some(Ok(Event::default().event(m.name).data(m.data))),
        // 受信が追いつかず取りこぼした場合はスキップする。
        // 次の tick で最新状態が届くため、UI への影響はない。
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(SSE_KEEPALIVE))
}

// ---------------------------------------------------------------- コマンド

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Args {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    meeting_id: Option<String>,
    #[serde(default)]
    input: Option<MeetingContextInput>,
    #[serde(default)]
    settings: Option<AppSettings>,
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    document: Option<MeetingDocument>,
    #[serde(default)]
    from_transcription: Option<bool>,
    /// ブラウザ版で録音を始めるときの表示名。
    #[serde(default)]
    client_label: Option<String>,
}

impl Args {
    fn meeting_id(&self) -> AppResult<String> {
        self.meeting_id
            .clone()
            .ok_or_else(|| AppError::Invalid("meetingId が指定されていません".into()))
    }
}

/// `POST /api/command/<name>` を対応する service 関数へ振り分ける。
///
/// 実処理は同期関数で、DB アクセスやファイル書き込みを含むため
/// `spawn_blocking` で実行し、非同期ランタイムを塞がないようにする。
async fn dispatch(
    State(ctx): State<ServerContext>,
    AxumPath(command): AxumPath<String>,
    body: Option<Json<Args>>,
) -> Result<Json<Value>, ApiError> {
    let args = body.map(|Json(a)| a).unwrap_or(Args {
        title: None,
        meeting_id: None,
        input: None,
        settings: None,
        model_id: None,
        document: None,
        from_transcription: None,
        client_label: None,
    });

    let state = ctx.state.clone();
    let result = tokio::task::spawn_blocking(move || run_command(&state, &command, args))
        .await
        .map_err(|e| ApiError(AppError::Other(format!("処理を実行できませんでした: {e}"))))??;

    Ok(Json(result))
}

fn to_value<T: serde::Serialize>(value: T) -> AppResult<Value> {
    serde_json::to_value(value).map_err(AppError::from)
}

fn run_command(state: &Arc<AppState>, command: &str, args: Args) -> AppResult<Value> {
    match command {
        // ---- 会議
        "create_meeting" => to_value(service::create_meeting(state, args.title)?),
        "get_home_stats" => to_value(service::get_home_stats(state)?),
        "list_meetings" => to_value(service::list_meetings(state)?),
        "get_meeting_detail" => to_value(service::get_meeting_detail(state, args.meeting_id()?)?),
        "save_meeting_context" => {
            let input = args
                .input
                .clone()
                .ok_or_else(|| AppError::Invalid("input が指定されていません".into()))?;
            to_value(service::save_meeting_context(
                state,
                args.meeting_id()?,
                input,
            )?)
        }
        "delete_meeting" => {
            service::delete_meeting(state, args.meeting_id()?)?;
            Ok(Value::Null)
        }

        // ---- 録音
        // ブラウザ版ではマイクの取得はブラウザ側で行うため、入力元は常に BrowserUpload。
        "start_recording" => to_value(service::start_recording(
            state,
            args.meeting_id()?,
            RecordingSource::BrowserUpload {
                client_label: args
                    .client_label
                    .clone()
                    .unwrap_or_else(|| "ブラウザのマイク".to_string()),
            },
        )?),
        "pause_recording" => to_value(service::pause_recording(state)?),
        "resume_recording" => to_value(service::resume_recording(state)?),
        "stop_recording" => to_value(service::stop_recording(state)?),
        "get_recording_state" => to_value(service::get_recording_state(state)),
        "get_recoverable_meetings" => to_value(service::get_recoverable_meetings(state)?),
        "recover_meeting" => to_value(service::recover_meeting(state, args.meeting_id()?)?),
        "check_disk_status" => to_value(service::check_disk_status(state)),
        // ブラウザ版ではサーバのマイクを使わないため、常に空を返す。
        "list_input_devices" => to_value(Vec::<crate::audio::devices::AudioInputDevice>::new()),

        // ---- 会議終了後の処理
        "run_pipeline" => {
            service::run_pipeline(state.clone(), args.meeting_id()?)?;
            Ok(Value::Null)
        }
        "retry_pipeline" => {
            service::retry_pipeline(
                state.clone(),
                args.meeting_id()?,
                args.from_transcription.unwrap_or(false),
            )?;
            Ok(Value::Null)
        }
        "get_pipeline_status" => to_value(service::get_pipeline_status(state, args.meeting_id()?)?),
        "get_transcript" => to_value(service::get_transcript(state, args.meeting_id()?)?),

        // ---- 成果物
        "list_meeting_files" => to_value(service::list_meeting_files(state, args.meeting_id()?)?),
        "read_meeting_document" => {
            let document = args
                .document
                .ok_or_else(|| AppError::Invalid("document が指定されていません".into()))?;
            to_value(service::read_meeting_document(
                state,
                args.meeting_id()?,
                document,
            )?)
        }

        // ---- 設定・環境
        "get_settings" => to_value(service::get_settings(state)),
        "update_settings" => {
            let settings = args
                .settings
                .clone()
                .ok_or_else(|| AppError::Invalid("settings が指定されていません".into()))?;
            to_value(service::update_settings(state, settings)?)
        }
        "reset_settings" => to_value(service::reset_settings(state)?),
        "get_system_info" => to_value(service::get_system_info(state)),
        "check_components" => to_value(service::check_components(state)?),

        // ---- 音声認識モデル
        "list_whisper_models" => to_value(service::list_whisper_models(state)),
        "download_whisper_model" => {
            let model_id = args
                .model_id
                .clone()
                .ok_or_else(|| AppError::Invalid("modelId が指定されていません".into()))?;
            service::start_model_download(state.clone(), model_id)?;
            Ok(Value::Null)
        }
        "cancel_model_download" => {
            service::cancel_model_download(state);
            Ok(Value::Null)
        }

        // ブラウザ版では保存先を選ぶダイアログが使えないため、
        // 書き出しは /api/download/... のダウンロードで行う。
        "export_meeting_document" | "export_meeting_audio" => Err(AppError::Invalid(
            "ブラウザ版ではダウンロード機能をご利用ください。".to_string(),
        )),

        other => Err(AppError::NotFound(format!("コマンド {other}"))),
    }
}

// ---------------------------------------------------------------- 録音アップロード

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChunkQuery {
    meeting_id: String,
}

/// ブラウザから 16kHz / mono / 16bit PCM のチャンクを受け取る。
///
/// 受け取り次第ディスクへ追記するため、途中で通信が切れても
/// そこまでの音声はファイルに残る。
async fn upload_chunk(
    State(ctx): State<ServerContext>,
    Query(query): Query<ChunkQuery>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let state = ctx.state.clone();
    let snapshot = tokio::task::spawn_blocking(move || {
        service::append_recording_chunk(&state, &query.meeting_id, &body)
    })
    .await
    .map_err(|e| ApiError(AppError::Other(format!("録音データを保存できません: {e}"))))??;

    Ok(Json(to_value(snapshot).map_err(ApiError)?))
}

// ---------------------------------------------------------------- ファイル配信

/// 録音音声をブラウザで再生できるように配信する。
async fn serve_audio(
    State(ctx): State<ServerContext>,
    AxumPath(meeting_id): AxumPath<String>,
) -> Result<Response, ApiError> {
    let state = ctx.state.clone();
    let path = tokio::task::spawn_blocking(move || service::audio_path_of(&state, &meeting_id))
        .await
        .map_err(|e| ApiError(AppError::Other(format!("音声を取得できません: {e}"))))??;

    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        ApiError(AppError::Io(format!(
            "音声ファイルを読み込めません ({}): {e}",
            path.display()
        )))
    })?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "audio/wav".parse().unwrap());
    Ok((headers, bytes).into_response())
}

/// 議事録・まとめ・文字起こし・音声をダウンロードさせる。
async fn download_document(
    State(ctx): State<ServerContext>,
    AxumPath((meeting_id, document)): AxumPath<(String, String)>,
) -> Result<Response, ApiError> {
    let state = ctx.state.clone();

    if document == "audio" {
        let path = tokio::task::spawn_blocking({
            let state = state.clone();
            let meeting_id = meeting_id.clone();
            move || service::audio_path_of(&state, &meeting_id)
        })
        .await
        .map_err(|e| ApiError(AppError::Other(format!("音声を取得できません: {e}"))))??;

        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| ApiError(AppError::Io(format!("音声を読み込めません: {e}"))))?;
        return Ok(attachment("audio.wav", "audio/wav", bytes));
    }

    let kind = match document.as_str() {
        "transcript" => MeetingDocument::Transcript,
        "minutes" => MeetingDocument::Minutes,
        "summary" => MeetingDocument::Summary,
        other => return Err(ApiError(AppError::NotFound(format!("成果物 {other}")))),
    };

    let content = tokio::task::spawn_blocking(move || {
        service::read_meeting_document(&state, meeting_id, kind)
    })
    .await
    .map_err(|e| ApiError(AppError::Other(format!("成果物を取得できません: {e}"))))??
    .ok_or_else(|| {
        ApiError(AppError::NotFound(
            "この成果物はまだ作成されていません".into(),
        ))
    })?;

    let filename = match kind {
        MeetingDocument::Transcript => "transcript.txt",
        MeetingDocument::Minutes => "minutes.md",
        MeetingDocument::Summary => "summary.md",
    };
    Ok(attachment(
        filename,
        "text/plain; charset=utf-8",
        content.into_bytes(),
    ))
}

fn attachment(filename: &str, content_type: &str, bytes: Vec<u8>) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
    if let Ok(value) = format!("attachment; filename=\"{filename}\"").parse() {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    (headers, bytes).into_response()
}
