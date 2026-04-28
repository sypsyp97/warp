//! Bridges `byo_provider`'s plain-text streaming output back to the
//! proprietary `warp_multi_agent_api::ResponseEvent` shape that the
//! existing UI consumers (`history_model::apply_client_actions`,
//! `conversation::apply_client_action`, etc.) already understand.
//!
//! Stock Warp's server is an agent runtime that emits a rich event
//! stream (CreateTask, AppendToMessageContent, tool calls, …); the
//! client is a thin renderer.  The slim fork replaces that runtime
//! with a direct LLM call, but synthesises the minimum subset of
//! events needed to drive the renderer for plain chat:
//!
//! ```text
//! StreamInit { conversation_id, request_id, run_id }
//! ClientActions [ BeginTransaction ]
//! ClientActions [ CreateTask { task: <fresh> } ]
//! ClientActions [ AddMessagesToTask { … empty AgentOutput message } ]
//! loop on Delta:
//!   ClientActions [ AppendToMessageContent { … message.agent_output.text += chunk … } ]
//! ClientActions [ CommitTransaction ]
//! StreamFinished { reason: Done | InvalidApiKey | QuotaLimit | … }
//! ```
//!
//! Tool use, file editing, MCP, multi-agent orchestration, todos,
//! artifacts — all degrade silently to no-op.  Restoring those is
//! Phase B of the Phase 2 rewrite.

use std::sync::Arc;

use byo_provider::{
    provider_for, ChatProvider, ChatStreamEvent, Message, ProviderConfig, ProviderError,
    ProviderKind, Role,
};
use futures::stream::{BoxStream, StreamExt};
use prost_types::FieldMask;
use uuid::Uuid;
use warp_multi_agent_api as api;

use crate::server::server_api::{AIApiError, AIOutputStream};

/// Drive a single BYO-provider chat round and yield faked
/// `ResponseEvent`s that look like Warp's multi-agent server.
///
/// Always returns `Ok(stream)`; configuration / network failures are
/// surfaced as a single `Finished { reason: ... }` event in-stream so
/// existing error-rendering paths kick in.
pub async fn run_byo_chat(
    request: &api::Request,
) -> Result<AIOutputStream<api::ResponseEvent>, Arc<AIApiError>> {
    let user_query = extract_user_query(request).unwrap_or_default();

    let config = match load_provider_config() {
        Ok(c) => c,
        Err(msg) => return Ok(error_stream(missing_config_finished(msg))),
    };

    let provider: Box<dyn ChatProvider> = provider_for(config.clone());
    let stream = adapt_chat_stream(user_query, config.system_prompt.clone(), provider).boxed();
    Ok(stream)
}

// ---------------------------------------------------------------------------
// Provider config (env-var loader; settings UI is phase 2 follow-up)
// ---------------------------------------------------------------------------

const ENV_PROVIDER: &str = "WARP_BYO_PROVIDER";
const ENV_API_KEY: &str = "WARP_BYO_API_KEY";
const ENV_MODEL: &str = "WARP_BYO_MODEL";
const ENV_BASE_URL: &str = "WARP_BYO_BASE_URL";
const ENV_SYSTEM: &str = "WARP_BYO_SYSTEM_PROMPT";

fn load_provider_config() -> Result<ProviderConfig, String> {
    let kind_raw = std::env::var(ENV_PROVIDER)
        .map_err(|_| format!("{ENV_PROVIDER} not set (expected 'anthropic' or 'openai')"))?;
    let kind = match kind_raw.to_ascii_lowercase().as_str() {
        "anthropic" => ProviderKind::Anthropic,
        "openai" | "openai-compatible" | "compatible" => ProviderKind::OpenAI,
        other => return Err(format!("{ENV_PROVIDER}={other:?} is not 'anthropic' or 'openai'")),
    };
    let api_key = std::env::var(ENV_API_KEY)
        .map_err(|_| format!("{ENV_API_KEY} not set"))?;
    let model = std::env::var(ENV_MODEL).unwrap_or_else(|_| match kind {
        ProviderKind::Anthropic => "claude-sonnet-4-5-20250929".to_string(),
        ProviderKind::OpenAI => "gpt-4o-mini".to_string(),
    });
    let base_url = std::env::var(ENV_BASE_URL).ok();
    let system_prompt = std::env::var(ENV_SYSTEM).ok();

    Ok(ProviderConfig {
        kind,
        api_key,
        base_url,
        model,
        system_prompt,
        max_tokens: 4096,
    })
}

// ---------------------------------------------------------------------------
// Request → user query extraction
// ---------------------------------------------------------------------------

fn extract_user_query(request: &api::Request) -> Option<String> {
    let input = request.input.as_ref()?;
    let r#type = input.r#type.as_ref()?;
    use api::request::input::Type;
    match r#type {
        // Modern path: UserInputs containing a list of inputs; the
        // first UserQuery is what the human typed.
        Type::UserInputs(user_inputs) => {
            for ui in &user_inputs.inputs {
                if let Some(api::request::input::user_inputs::user_input::Input::UserQuery(uq)) =
                    ui.input.as_ref()
                {
                    return Some(uq.query.clone());
                }
            }
            None
        }
        // Deprecated direct UserQuery still in the wire format.
        Type::UserQuery(uq) => Some(uq.query.clone()),
        // Anything else (passive suggestions, code review, etc.) is
        // out of scope for plain BYO chat.  Caller falls through to
        // empty query.
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Stream adaptation
// ---------------------------------------------------------------------------

fn adapt_chat_stream(
    user_query: String,
    _system_prompt: Option<String>,
    provider: Box<dyn ChatProvider>,
) -> BoxStream<'static, Result<api::ResponseEvent, Arc<AIApiError>>> {
    use async_stream::stream;

    let s = stream! {
        let conversation_id = Uuid::new_v4().to_string();
        let request_id = Uuid::new_v4().to_string();
        let task_id = Uuid::new_v4().to_string();
        let assistant_msg_id = Uuid::new_v4().to_string();

        // --- 1. StreamInit -------------------------------------------------
        yield Ok(api::ResponseEvent {
            r#type: Some(api::response_event::Type::Init(
                api::response_event::StreamInit {
                    conversation_id,
                    request_id,
                    run_id: String::new(),
                },
            )),
        });

        // --- 2. BeginTransaction ------------------------------------------
        yield Ok(wrap_action(api::client_action::Action::BeginTransaction(
            api::client_action::BeginTransaction {},
        )));

        // --- 3. CreateTask ------------------------------------------------
        yield Ok(wrap_action(api::client_action::Action::CreateTask(
            api::client_action::CreateTask {
                task: Some(api::Task {
                    id: task_id.clone(),
                    description: String::new(),
                    dependencies: Some(api::task::Dependencies {
                        parent_task_id: String::new(),
                    }),
                    messages: Vec::new(),
                    summary: String::new(),
                    server_data: String::new(),
                }),
            },
        )));

        // --- 4. AddMessagesToTask: an empty AgentOutput we'll append to ---
        yield Ok(wrap_action(api::client_action::Action::AddMessagesToTask(
            api::client_action::AddMessagesToTask {
                task_id: task_id.clone(),
                messages: vec![empty_agent_message(&assistant_msg_id, &task_id)],
            },
        )));

        // --- 5. Stream chunks ---------------------------------------------
        let messages = vec![Message {
            role: Role::User,
            content: user_query,
        }];
        let stream_result = provider.stream_chat(messages).await;
        let mut stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                yield Ok(make_finished_for_error(&e));
                return;
            }
        };
        let mut errored = false;
        while let Some(evt) = stream.next().await {
            match evt {
                Ok(ChatStreamEvent::Delta(text)) if !text.is_empty() => {
                    yield Ok(wrap_action(api::client_action::Action::AppendToMessageContent(
                        api::client_action::AppendToMessageContent {
                            task_id: task_id.clone(),
                            message: Some(text_message(&assistant_msg_id, &task_id, text)),
                            mask: Some(FieldMask {
                                paths: vec!["agent_output.text".to_string()],
                            }),
                        },
                    )));
                }
                Ok(ChatStreamEvent::Delta(_)) => {} // empty delta — ignore
                Ok(ChatStreamEvent::Stop { .. }) => break,
                Err(e) => {
                    errored = true;
                    yield Ok(make_finished_for_error(&e));
                    break;
                }
            }
        }

        if !errored {
            // --- 6. CommitTransaction -------------------------------------
            yield Ok(wrap_action(api::client_action::Action::CommitTransaction(
                api::client_action::CommitTransaction {},
            )));

            // --- 7. Finished: Done ----------------------------------------
            yield Ok(make_finished_done());
        }
    };

    s.boxed()
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn wrap_action(action: api::client_action::Action) -> api::ResponseEvent {
    api::ResponseEvent {
        r#type: Some(api::response_event::Type::ClientActions(
            api::response_event::ClientActions {
                actions: vec![api::ClientAction {
                    action: Some(action),
                }],
            },
        )),
    }
}

fn empty_agent_message(message_id: &str, task_id: &str) -> api::Message {
    api::Message {
        id: message_id.to_string(),
        task_id: task_id.to_string(),
        request_id: String::new(),
        timestamp: None,
        server_message_data: String::new(),
        citations: Vec::new(),
        message: Some(api::message::Message::AgentOutput(
            api::message::AgentOutput {
                text: String::new(),
            },
        )),
    }
}

fn text_message(message_id: &str, task_id: &str, text: String) -> api::Message {
    api::Message {
        id: message_id.to_string(),
        task_id: task_id.to_string(),
        request_id: String::new(),
        timestamp: None,
        server_message_data: String::new(),
        citations: Vec::new(),
        message: Some(api::message::Message::AgentOutput(
            api::message::AgentOutput { text },
        )),
    }
}

fn make_finished_done() -> api::ResponseEvent {
    use api::response_event::stream_finished as sf;
    api::ResponseEvent {
        r#type: Some(api::response_event::Type::Finished(
            api::response_event::StreamFinished {
                reason: Some(sf::Reason::Done(sf::Done {})),
                token_usage: Vec::new(),
                should_refresh_model_config: false,
                request_cost: None,
                conversation_usage_metadata: None,
            },
        )),
    }
}

fn make_finished_for_error(e: &ProviderError) -> api::ResponseEvent {
    use api::response_event::stream_finished as sf;
    let reason = match e {
        ProviderError::InvalidApiKey { .. } => sf::Reason::InvalidApiKey(sf::InvalidApiKey {
            provider: api::LlmProvider::Unknown.into(),
            model_name: String::new(),
        }),
        ProviderError::QuotaLimit => sf::Reason::QuotaLimit(sf::QuotaLimit {}),
        ProviderError::Unavailable { .. } => sf::Reason::LlmUnavailable(sf::LlmUnavailable {}),
        ProviderError::ContextWindowExceeded => {
            sf::Reason::ContextWindowExceeded(sf::ContextWindowExceeded {})
        }
        _ => sf::Reason::InternalError(sf::InternalError {
            message: e.to_string(),
        }),
    };
    api::ResponseEvent {
        r#type: Some(api::response_event::Type::Finished(
            api::response_event::StreamFinished {
                reason: Some(reason),
                token_usage: Vec::new(),
                should_refresh_model_config: false,
                request_cost: None,
                conversation_usage_metadata: None,
            },
        )),
    }
}

fn missing_config_finished(detail: String) -> api::ResponseEvent {
    use api::response_event::stream_finished as sf;
    api::ResponseEvent {
        r#type: Some(api::response_event::Type::Finished(
            api::response_event::StreamFinished {
                reason: Some(sf::Reason::InvalidApiKey(sf::InvalidApiKey {
                    provider: api::LlmProvider::Unknown.into(),
                    model_name: detail,
                })),
                token_usage: Vec::new(),
                should_refresh_model_config: false,
                request_cost: None,
                conversation_usage_metadata: None,
            },
        )),
    }
}

fn error_stream(event: api::ResponseEvent) -> AIOutputStream<api::ResponseEvent> {
    futures::stream::once(async move { Ok(event) }).boxed()
}
