use crate::bridge::{Bridge, StreamHandle};
use crate::prompt::conversation_prompt;
use async_stream::stream;
use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub bridge: Bridge,
    pub default_timeout: u64,
}

pub async fn start_http_server(state: AppState, addr: &str) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/models", get(list_models))
        .route("/health", get(health))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("HTTP provider listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

// ─── Request / Response types ───────────────────────────────

#[derive(Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatRequest {
    model: Option<String>,
    messages: Vec<ChatMessage>,
    stream: Option<bool>,
}

#[derive(Serialize)]
struct ChatResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<ChatChoice>,
    usage: Usage,
}

#[derive(Serialize)]
struct ChatChoice {
    index: u32,
    message: ChatMessageOut,
    finish_reason: String,
}

#[derive(Serialize)]
struct ChatMessageOut {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Serialize)]
struct ChatChunk {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<ChatChunkChoice>,
}

#[derive(Serialize)]
struct ChatChunkChoice {
    index: u32,
    delta: Delta,
    finish_reason: Option<String>,
}

#[derive(Serialize)]
struct Delta {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    message: String,
    r#type: String,
}

// ─── Handlers ───────────────────────────────────────────────

async fn chat_completions(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Response {
    if !state.bridge.is_connected().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: ErrorDetail {
                    message: "ChatGPT is not connected (Chrome not running).".to_string(),
                    r#type: "server_error".to_string(),
                },
            }),
        )
            .into_response();
    }

    let messages: Vec<(String, String)> = req
        .messages
        .iter()
        .map(|m| (m.role.clone(), m.content.clone()))
        .collect();

    let prompt = conversation_prompt(&messages);
    let model = req.model.clone();
    let timeout = state.default_timeout;
    let format = "markdown".to_string();

    if req.stream.unwrap_or(false) {
        streaming_response(state, prompt, model, timeout, format).await
    } else {
        non_streaming_response(state, prompt, model, timeout, format).await
    }
}

async fn non_streaming_response(
    state: AppState,
    prompt: String,
    model: Option<String>,
    timeout: u64,
    format: String,
) -> Response {
    match state
        .bridge
        .request(
            prompt,
            timeout,
            model.clone(),
            Some(format),
        )
        .await
    {
        Ok(text) => {
            let response = ChatResponse {
                id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                object: "chat.completion".to_string(),
                created: now_unix(),
                model: model.unwrap_or_else(|| "chatgpt".to_string()),
                choices: vec![ChatChoice {
                    index: 0,
                    message: ChatMessageOut {
                        role: "assistant".to_string(),
                        content: text,
                    },
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                error: ErrorDetail {
                    message: e.to_string(),
                    r#type: "upstream_error".to_string(),
                },
            }),
        )
            .into_response(),
    }
}

async fn streaming_response(
    state: AppState,
    prompt: String,
    model: Option<String>,
    timeout: u64,
    format: String,
) -> Response {
    let handle = match state
        .bridge
        .request_streaming(
            prompt,
            timeout,
            model.clone(),
            Some(format),
        )
        .await
    {
        Ok(h) => h,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: ErrorDetail {
                        message: e.to_string(),
                        r#type: "upstream_error".to_string(),
                    },
                }),
            )
                .into_response();
        }
    };

    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let created = now_unix();
    let model_name = model.unwrap_or_else(|| "chatgpt".to_string());
    let timeout_dur = Duration::from_secs(timeout.saturating_add(30));

    let s = streaming_sse(handle, id, created, model_name, timeout_dur);
    Sse::new(s)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn streaming_sse(
    handle: StreamHandle,
    id: String,
    created: u64,
    model_name: String,
    timeout_dur: Duration,
) -> impl futures::Stream<Item = Result<Event, std::convert::Infallible>> {
    stream! {
        let first_chunk = ChatChunk {
            id: id.clone(),
            object: "chat.completion.chunk".to_string(),
            created,
            model: model_name.clone(),
            choices: vec![ChatChunkChoice {
                index: 0,
                delta: Delta {
                    role: Some("assistant".to_string()),
                    content: None,
                },
                finish_reason: None,
            }],
        };
        yield Ok(Event::default().data(
            serde_json::to_string(&first_chunk).unwrap_or_default(),
        ));

        let mut handle = handle;
        let mut prev_text = String::new();

        loop {
            tokio::select! {
                Ok(partial) = handle.partials.recv() => {
                    let delta = if partial.starts_with(&prev_text) {
                        &partial[prev_text.len()..]
                    } else {
                        &partial[..]
                    };
                    if !delta.is_empty() {
                        let chunk = ChatChunk {
                            id: id.clone(),
                            object: "chat.completion.chunk".to_string(),
                            created,
                            model: model_name.clone(),
                            choices: vec![ChatChunkChoice {
                                index: 0,
                                delta: Delta {
                                    role: None,
                                    content: Some(delta.to_string()),
                                },
                                finish_reason: None,
                            }],
                        };
                        yield Ok(Event::default().data(
                            serde_json::to_string(&chunk).unwrap_or_default(),
                        ));
                    }
                    prev_text = partial;
                }
                result = &mut handle.result => {
                    match result {
                        Ok(Ok(r)) => {
                            let delta = r.strip_prefix(&prev_text).unwrap_or(&r);
                            if !delta.is_empty() {
                                let chunk = ChatChunk {
                                    id: id.clone(),
                                    object: "chat.completion.chunk".to_string(),
                                    created,
                                    model: model_name.clone(),
                                    choices: vec![ChatChunkChoice {
                                        index: 0,
                                        delta: Delta {
                                            role: None,
                                            content: Some(delta.to_string()),
                                        },
                                        finish_reason: None,
                                    }],
                                };
                                yield Ok(Event::default().data(
                                    serde_json::to_string(&chunk).unwrap_or_default(),
                                ));
                            }
                            let finish_chunk = ChatChunk {
                                id: id.clone(),
                                object: "chat.completion.chunk".to_string(),
                                created,
                                model: model_name.clone(),
                                choices: vec![ChatChunkChoice {
                                    index: 0,
                                    delta: Delta {
                                        role: None,
                                        content: None,
                                    },
                                    finish_reason: Some("stop".to_string()),
                                }],
                            };
                            yield Ok(Event::default().data(
                                serde_json::to_string(&finish_chunk).unwrap_or_default(),
                            ));
                            yield Ok(Event::default().data("[DONE]"));
                            break;
                        }
                        Ok(Err(e)) => {
                            let err = serde_json::json!({
                                "error": {"message": e.to_string(), "type": "upstream_error"}
                            });
                            yield Ok(Event::default().data(err.to_string()));
                            break;
                        }
                        Err(_) => {
                            let err = serde_json::json!({
                                "error": {"message": "stream closed unexpectedly", "type": "server_error"}
                            });
                            yield Ok(Event::default().data(err.to_string()));
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(timeout_dur) => {
                    let err = serde_json::json!({
                        "error": {"message": "stream timeout", "type": "timeout"}
                    });
                    yield Ok(Event::default().data(err.to_string()));
                    break;
                }
            }
        }
    }
}

async fn list_models() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "object": "list",
        "data": [
            {"id": "gpt-4o", "object": "model", "created": 0, "owned_by": "openai"},
            {"id": "gpt-4o-mini", "object": "model", "created": 0, "owned_by": "openai"},
            {"id": "o1", "object": "model", "created": 0, "owned_by": "openai"},
            {"id": "o1-mini", "object": "model", "created": 0, "owned_by": "openai"},
            {"id": "o1-preview", "object": "model", "created": 0, "owned_by": "openai"},
            {"id": "gpt-4-turbo", "object": "model", "created": 0, "owned_by": "openai"},
            {"id": "gpt-4", "object": "model", "created": 0, "owned_by": "openai"},
            {"id": "gpt-3.5-turbo", "object": "model", "created": 0, "owned_by": "openai"}
        ]
    }))
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    let connected = state.bridge.is_connected().await;
    Json(serde_json::json!({
        "status": "ok",
        "connected": connected,
        "mode": "temporary_chat"
    }))
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
