//! Bring-your-own LLM provider clients for the slim fork.
//!
//! Stock Warp routes every Agent Mode request through its own server
//! (`/ai/multi-agent`) using a proprietary protobuf protocol — even
//! when the user supplies their own API key, the key is forwarded
//! to Warp's relay. The slim fork bypasses that relay and talks
//! directly to the upstream provider chosen by the user.
//!
//! # Phased roadmap
//!
//! 1. **Plain chat** (this crate's initial scope): `ChatProvider`
//!    streams text deltas from a single user message.  The Warp
//!    `ResponseEvent` consumer in `app/src/ai/blocklist/controller`
//!    is fed faked `ClientActions::TextDelta` events so the existing
//!    UI continues to render the assistant's reply.  Tool use, file
//!    editing, MCP, multi-agent orchestration, and `passive-suggestions`
//!    all degrade to no-op.
//!
//! 2. **Tool use loop** (later): wire Anthropic / OpenAI tool calls
//!    onto Warp's existing tool registry so Agent Mode comes back —
//!    file read/write, run_command, grep, etc.  This will require a
//!    second trait surface (`AgentProvider`) on top of `ChatProvider`.
//!
//! # Supported providers
//!
//! - `AnthropicProvider` — official Messages API (`api.anthropic.com`).
//! - `OpenAIProvider` — Chat Completions API; works against any
//!   OpenAI-compatible endpoint (OpenAI itself, Azure OpenAI,
//!   Anthropic compat endpoint, Ollama, vLLM, OpenRouter, etc.) by
//!   overriding `base_url`.
//!
//! # Wire formats
//!
//! Both providers use server-sent events for streaming.  Anthropic
//! emits typed events (`message_start`, `content_block_delta`, etc.);
//! OpenAI emits a single `data:` stream with chat completion chunks.
//! The shared output type [`ChatStreamEvent`] hides this difference
//! from callers.

use std::pin::Pin;

use async_trait::async_trait;
use futures::stream::{BoxStream, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Provider selection — which API surface to talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Anthropic Messages API (`https://api.anthropic.com/v1/messages`).
    Anthropic,
    /// OpenAI Chat Completions API.  Default base URL
    /// `https://api.openai.com/v1`; override `base_url` to target any
    /// OpenAI-compatible server (Azure, Ollama, vLLM, OpenRouter, ...).
    OpenAI,
    /// ChatGPT subscription backend used by the Codex CLI: OAuth access
    /// token (no API key), Responses API (`/codex/responses`), and a
    /// distinct streaming event shape (`response.output_text.delta`).
    /// `api_key` carries the OAuth access token; `base_url` defaults to
    /// `https://chatgpt.com/backend-api`.
    ChatGpt,
}

/// User-configured connection parameters.  Persisted under
/// `agents.byo_provider.*` in the settings TOML once Settings UI is wired.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub api_key: String,
    /// Override the provider's default base URL.  None = use built-in
    /// default.  Trailing slash is stripped.
    #[serde(default)]
    pub base_url: Option<String>,
    /// Model name passed verbatim to the provider.
    pub model: String,
    /// Optional system prompt sent on every request.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Hard cap on output tokens (Anthropic requires this; OpenAI
    /// treats it as `max_tokens`).
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_max_tokens() -> u32 {
    4096
}

/// One turn in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

/// One streaming output event.  Both Anthropic and OpenAI streams are
/// normalized into this shape.
#[derive(Debug, Clone)]
pub enum ChatStreamEvent {
    /// A chunk of assistant text.  Concatenate all `Delta` payloads
    /// (in order) to reconstruct the full assistant turn.
    Delta(String),
    /// Stream finished cleanly.
    Stop {
        /// Why the model stopped.  Loosely modeled on Anthropic's
        /// `stop_reason` and OpenAI's `finish_reason`; not all values
        /// are emitted by every provider.
        reason: StopReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// Natural end of message.
    EndTurn,
    /// Hit `max_tokens`.
    MaxTokens,
    /// Hit a stop sequence configured on the request (rare for our use).
    StopSequence,
    /// Provider returned an unrecognized reason — falls through here.
    Unknown,
}

/// Errors a provider call can produce.  Mirrors the variants the
/// existing `StreamFinished::Reason` consumer in
/// `app/src/ai/blocklist/controller.rs:2557` understands so the
/// adapter layer can map cleanly.
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("invalid API key (status {status})")]
    InvalidApiKey { status: u16, message: String },
    #[error("rate limited / quota exceeded")]
    QuotaLimit,
    #[error("provider unavailable / 5xx ({status})")]
    Unavailable { status: u16, message: String },
    #[error("context window exceeded")]
    ContextWindowExceeded,
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("malformed event-stream payload: {0}")]
    Malformed(String),
    #[error("other: {0}")]
    Other(#[from] anyhow::Error),
}

pub type ChatStream = BoxStream<'static, Result<ChatStreamEvent, ProviderError>>;

/// The trait `app/src/server/server_api.rs` will dispatch through.
/// Implementations build the provider-specific request body, POST,
/// parse the SSE stream, and yield normalized [`ChatStreamEvent`]s.
///
/// Tool use is intentionally absent from this trait — it's added in
/// phase B alongside `AgentProvider`.
#[async_trait]
pub trait ChatProvider: Send + Sync {
    async fn stream_chat(&self, messages: Vec<Message>) -> Result<ChatStream, ProviderError>;
}

// ---------------------------------------------------------------------------
// Anthropic provider
// ---------------------------------------------------------------------------

const ANTHROPIC_DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION_HEADER: &str = "anthropic-version";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    config: ProviderConfig,
    http: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(config: ProviderConfig) -> Self {
        debug_assert_eq!(config.kind, ProviderKind::Anthropic);
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct AnthropicRequestBody<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<AnthropicWireMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    stream: bool,
}

#[derive(Serialize)]
struct AnthropicWireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

/// Subset of Anthropic's typed SSE events we care about for plain
/// text streaming.  See:
/// <https://docs.anthropic.com/en/api/messages-streaming#event-types>.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
enum AnthropicSseEvent {
    MessageStart { message: AnthropicMessageMeta },
    ContentBlockDelta { delta: AnthropicDelta },
    MessageDelta { delta: AnthropicMessageStopMeta },
    MessageStop,
    /// Other events (`content_block_start`, `content_block_stop`,
    /// `ping`) are intentionally swallowed.
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct AnthropicMessageMeta {
    id: String,
    model: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicDelta {
    TextDelta { text: String },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct AnthropicMessageStopMeta {
    stop_reason: Option<String>,
}

#[async_trait]
impl ChatProvider for AnthropicProvider {
    async fn stream_chat(&self, messages: Vec<Message>) -> Result<ChatStream, ProviderError> {
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or(ANTHROPIC_DEFAULT_BASE_URL)
            .trim_end_matches('/');
        let url = format!("{base}/v1/messages");

        let wire_messages: Vec<_> = messages
            .iter()
            .map(|m| AnthropicWireMessage {
                role: match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                },
                content: m.content.as_str(),
            })
            .collect();

        let body = AnthropicRequestBody {
            model: &self.config.model,
            max_tokens: self.config.max_tokens,
            messages: wire_messages,
            system: self.config.system_prompt.as_deref(),
            stream: true,
        };

        let response = self
            .http
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header(ANTHROPIC_VERSION_HEADER, ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let response = check_response_status(response).await?;
        let bytes = response.bytes_stream();
        Ok(parse_anthropic_sse(bytes).boxed())
    }
}

fn parse_anthropic_sse(
    bytes: impl Stream<Item = reqwest::Result<bytes::Bytes>> + Send + 'static,
) -> impl Stream<Item = Result<ChatStreamEvent, ProviderError>> + Send {
    SseLines::new(bytes).filter_map(|line| async move {
        let line = match line {
            Ok(l) => l,
            Err(e) => return Some(Err(e)),
        };
        let payload = line.strip_prefix("data: ")?;
        if payload == "[DONE]" {
            return Some(Ok(ChatStreamEvent::Stop {
                reason: StopReason::EndTurn,
            }));
        }
        match serde_json::from_str::<AnthropicSseEvent>(payload) {
            Ok(AnthropicSseEvent::ContentBlockDelta {
                delta: AnthropicDelta::TextDelta { text },
            }) => Some(Ok(ChatStreamEvent::Delta(text))),
            Ok(AnthropicSseEvent::MessageDelta { delta }) => Some(Ok(ChatStreamEvent::Stop {
                reason: map_stop_reason(delta.stop_reason.as_deref()),
            })),
            Ok(AnthropicSseEvent::MessageStop) => Some(Ok(ChatStreamEvent::Stop {
                reason: StopReason::EndTurn,
            })),
            Ok(_) => None,
            Err(e) => Some(Err(ProviderError::Malformed(format!(
                "anthropic sse: {e} ({payload})"
            )))),
        }
    })
}

fn map_stop_reason(s: Option<&str>) -> StopReason {
    match s {
        Some("end_turn") | Some("stop") => StopReason::EndTurn,
        Some("max_tokens") | Some("length") => StopReason::MaxTokens,
        Some("stop_sequence") => StopReason::StopSequence,
        _ => StopReason::Unknown,
    }
}

// ---------------------------------------------------------------------------
// OpenAI / OpenAI-compatible provider
// ---------------------------------------------------------------------------

const OPENAI_DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

pub struct OpenAIProvider {
    config: ProviderConfig,
    http: reqwest::Client,
}

impl OpenAIProvider {
    pub fn new(config: ProviderConfig) -> Self {
        debug_assert_eq!(config.kind, ProviderKind::OpenAI);
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct OpenAIRequestBody<'a> {
    model: &'a str,
    messages: Vec<OpenAIWireMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Serialize)]
struct OpenAIWireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OpenAIChunk {
    choices: Vec<OpenAIChoice>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    #[serde(default)]
    delta: OpenAIChoiceDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct OpenAIChoiceDelta {
    #[serde(default)]
    content: Option<String>,
}

#[async_trait]
impl ChatProvider for OpenAIProvider {
    async fn stream_chat(&self, messages: Vec<Message>) -> Result<ChatStream, ProviderError> {
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or(OPENAI_DEFAULT_BASE_URL)
            .trim_end_matches('/');
        let url = format!("{base}/chat/completions");

        // Prepend system prompt as a leading `system` message (OpenAI
        // doesn't have a separate field).
        let mut wire_messages = Vec::with_capacity(messages.len() + 1);
        if let Some(sys) = self.config.system_prompt.as_deref() {
            wire_messages.push(OpenAIWireMessage {
                role: "system",
                content: sys,
            });
        }
        wire_messages.extend(messages.iter().map(|m| OpenAIWireMessage {
            role: match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            },
            content: m.content.as_str(),
        }));

        let body = OpenAIRequestBody {
            model: &self.config.model,
            messages: wire_messages,
            max_tokens: Some(self.config.max_tokens),
            stream: true,
        };

        let response = self
            .http
            .post(&url)
            .header("authorization", format!("Bearer {}", self.config.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let response = check_response_status(response).await?;
        let bytes = response.bytes_stream();
        Ok(parse_openai_sse(bytes).boxed())
    }
}

fn parse_openai_sse(
    bytes: impl Stream<Item = reqwest::Result<bytes::Bytes>> + Send + 'static,
) -> impl Stream<Item = Result<ChatStreamEvent, ProviderError>> + Send {
    SseLines::new(bytes).filter_map(|line| async move {
        let line = match line {
            Ok(l) => l,
            Err(e) => return Some(Err(e)),
        };
        let payload = line.strip_prefix("data: ")?;
        if payload == "[DONE]" {
            return Some(Ok(ChatStreamEvent::Stop {
                reason: StopReason::EndTurn,
            }));
        }
        match serde_json::from_str::<OpenAIChunk>(payload) {
            Ok(chunk) => {
                let choice = chunk.choices.into_iter().next()?;
                if let Some(reason) = choice.finish_reason {
                    return Some(Ok(ChatStreamEvent::Stop {
                        reason: map_stop_reason(Some(reason.as_str())),
                    }));
                }
                let text = choice.delta.content?;
                if text.is_empty() {
                    None
                } else {
                    Some(Ok(ChatStreamEvent::Delta(text)))
                }
            }
            Err(e) => Some(Err(ProviderError::Malformed(format!(
                "openai sse: {e} ({payload})"
            )))),
        }
    })
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

async fn check_response_status(
    response: reqwest::Response,
) -> Result<reqwest::Response, ProviderError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let status_code = status.as_u16();
    let body = response.text().await.unwrap_or_default();
    Err(match status_code {
        401 | 403 => ProviderError::InvalidApiKey {
            status: status_code,
            message: body,
        },
        429 => ProviderError::QuotaLimit,
        500..=599 => ProviderError::Unavailable {
            status: status_code,
            message: body,
        },
        _ => ProviderError::Other(anyhow::anyhow!("http {status_code}: {body}")),
    })
}

/// SSE line splitter: takes a byte stream, yields one logical event
/// line at a time (stripping trailing `\n` and ignoring `\r`).
struct SseLines<S> {
    inner: Pin<Box<S>>,
    buffer: Vec<u8>,
}

impl<S> SseLines<S>
where
    S: Stream<Item = reqwest::Result<bytes::Bytes>> + Send + 'static,
{
    fn new(inner: S) -> Self {
        Self {
            inner: Box::pin(inner),
            buffer: Vec::new(),
        }
    }
}

impl<S> Stream for SseLines<S>
where
    S: Stream<Item = reqwest::Result<bytes::Bytes>> + Send + 'static,
{
    type Item = Result<String, ProviderError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;

        loop {
            // Emit any complete line already in the buffer.
            if let Some(idx) = self.buffer.iter().position(|&b| b == b'\n') {
                let line_bytes: Vec<u8> = self.buffer.drain(..=idx).collect();
                let trimmed = line_bytes
                    .split_last()
                    .map(|(_, rest)| rest)
                    .unwrap_or(&[])
                    .iter()
                    .copied()
                    .filter(|&b| b != b'\r')
                    .collect::<Vec<u8>>();
                let line = match String::from_utf8(trimmed) {
                    Ok(s) => s,
                    Err(e) => {
                        return Poll::Ready(Some(Err(ProviderError::Malformed(format!(
                            "non-utf8 sse line: {e}"
                        )))));
                    }
                };
                if line.is_empty() {
                    // Heartbeat / event boundary; drop and keep reading.
                    continue;
                }
                return Poll::Ready(Some(Ok(line)));
            }

            // Need more bytes from upstream.
            match self.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    self.buffer.extend_from_slice(&chunk);
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(ProviderError::Transport(e))));
                }
                Poll::Ready(None) => {
                    if self.buffer.is_empty() {
                        return Poll::Ready(None);
                    }
                    // Flush trailing partial line.
                    let trailing = std::mem::take(&mut self.buffer);
                    let line = match String::from_utf8(trailing) {
                        Ok(s) if !s.is_empty() => s,
                        _ => return Poll::Ready(None),
                    };
                    return Poll::Ready(Some(Ok(line)));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Convenience
// ---------------------------------------------------------------------------

/// Build the right `ChatProvider` from a config.
pub fn provider_for(config: ProviderConfig) -> Box<dyn ChatProvider> {
    match config.kind {
        ProviderKind::Anthropic => Box::new(AnthropicProvider::new(config)),
        ProviderKind::OpenAI => Box::new(OpenAIProvider::new(config)),
        ProviderKind::ChatGpt => Box::new(ChatGptProvider::new(config)),
    }
}

// ---------------------------------------------------------------------------
// ChatGPT subscription backend (Codex CLI)
// ---------------------------------------------------------------------------

const CHATGPT_DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api";

pub struct ChatGptProvider {
    config: ProviderConfig,
    http: reqwest::Client,
}

impl ChatGptProvider {
    pub fn new(config: ProviderConfig) -> Self {
        debug_assert_eq!(config.kind, ProviderKind::ChatGpt);
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct ChatGptRequestBody<'a> {
    model: &'a str,
    input: Vec<ChatGptInputMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<&'a str>,
    stream: bool,
    store: bool,
}

#[derive(Serialize)]
struct ChatGptInputMessage<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    role: &'a str,
    content: Vec<ChatGptInputContent<'a>>,
}

#[derive(Serialize)]
struct ChatGptInputContent<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'a str,
}

/// Subset of the Responses-API streamed events we care about for plain
/// text streaming. The full event set includes tool-call deltas, audio,
/// reasoning, etc. — we ignore them.
#[derive(Deserialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
enum ChatGptSseEvent {
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta { delta: String },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {},
    #[serde(rename = "response.completed")]
    Completed { response: ChatGptCompletedMeta },
    #[serde(rename = "response.failed")]
    Failed {
        #[serde(default)]
        response: ChatGptFailedMeta,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ChatGptCompletedMeta {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    incomplete_details: Option<ChatGptIncompleteDetails>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ChatGptIncompleteDetails {
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Deserialize, Default)]
#[allow(dead_code)]
struct ChatGptFailedMeta {
    #[serde(default)]
    error: Option<ChatGptError>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ChatGptError {
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

#[async_trait]
impl ChatProvider for ChatGptProvider {
    async fn stream_chat(&self, messages: Vec<Message>) -> Result<ChatStream, ProviderError> {
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or(CHATGPT_DEFAULT_BASE_URL)
            .trim_end_matches('/');
        let url = format!("{base}/codex/responses");

        let wire_messages: Vec<_> = messages
            .iter()
            .map(|m| ChatGptInputMessage {
                kind: "message",
                role: match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                },
                content: vec![ChatGptInputContent {
                    kind: match m.role {
                        Role::User => "input_text",
                        Role::Assistant => "output_text",
                    },
                    text: m.content.as_str(),
                }],
            })
            .collect();

        let body = ChatGptRequestBody {
            model: &self.config.model,
            input: wire_messages,
            instructions: self.config.system_prompt.as_deref(),
            stream: true,
            // ChatGPT subscription quotas don't support server-side
            // conversation persistence; we manage history client-side.
            store: false,
        };

        let response = self
            .http
            .post(&url)
            .header(
                "authorization",
                format!("Bearer {}", self.config.api_key),
            )
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            // Codex CLI sends this; the backend uses it to disambiguate
            // the originator. Harmless if the server ignores it.
            .header("originator", "warp_slim_byo")
            .json(&body)
            .send()
            .await?;

        let response = check_response_status(response).await?;
        let bytes = response.bytes_stream();
        Ok(parse_chatgpt_sse(bytes).boxed())
    }
}

fn parse_chatgpt_sse(
    bytes: impl Stream<Item = reqwest::Result<bytes::Bytes>> + Send + 'static,
) -> impl Stream<Item = Result<ChatStreamEvent, ProviderError>> + Send {
    SseLines::new(bytes).filter_map(|line| async move {
        let line = match line {
            Ok(l) => l,
            Err(e) => return Some(Err(e)),
        };
        let payload = line.strip_prefix("data: ")?;
        if payload == "[DONE]" {
            return Some(Ok(ChatStreamEvent::Stop {
                reason: StopReason::EndTurn,
            }));
        }
        match serde_json::from_str::<ChatGptSseEvent>(payload) {
            Ok(ChatGptSseEvent::OutputTextDelta { delta }) if !delta.is_empty() => {
                Some(Ok(ChatStreamEvent::Delta(delta)))
            }
            Ok(ChatGptSseEvent::Completed { response }) => {
                let reason = response
                    .incomplete_details
                    .as_ref()
                    .and_then(|d| d.reason.as_deref())
                    .map(|r| match r {
                        "max_output_tokens" => StopReason::MaxTokens,
                        _ => StopReason::Unknown,
                    })
                    .unwrap_or(StopReason::EndTurn);
                Some(Ok(ChatStreamEvent::Stop { reason }))
            }
            Ok(ChatGptSseEvent::Failed { response }) => {
                let msg = response
                    .error
                    .and_then(|e| e.message)
                    .unwrap_or_else(|| "ChatGPT backend reported failure".to_string());
                Some(Err(ProviderError::Other(anyhow::anyhow!(msg))))
            }
            Ok(_) => None,
            Err(e) => Some(Err(ProviderError::Malformed(format!(
                "chatgpt sse: {e} ({payload})"
            )))),
        }
    })
}
