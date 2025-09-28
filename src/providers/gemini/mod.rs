use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_stream::stream;
use axum::{
    body::Body,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use colored::Colorize;
use futures::{FutureExt, Stream, StreamExt, pin_mut};
use snafu::{GenerateImplicitData, Location};
use tokio::select;
use tracing::info;
use yup_oauth2::ServiceAccountKey;

use crate::{
    config::CLEWDR_CONFIG,
    error::ClewdrError,
    gemini_state::{GeminiApiFormat, GeminiState},
    middleware::gemini::GeminiContext,
    services::key_actor::KeyActorHandle,
    types::{gemini::request::GeminiRequestBody, oai::CreateMessageParams},
    utils::enabled,
};

use super::LLMProvider;

#[derive(Clone)]
pub enum GeminiPayload {
    Native(GeminiRequestBody),
    OpenAI(CreateMessageParams),
}

#[derive(Clone)]
pub struct GeminiInvocation {
    pub payload: GeminiPayload,
    pub context: GeminiContext,
}

#[derive(Clone)]
pub struct GeminiProviders {
    ai_studio: Arc<GeminiAiStudioProvider>,
    vertex: Arc<GeminiVertexProvider>,
}

impl GeminiProviders {
    pub fn new(key_actor_handle: KeyActorHandle) -> Self {
        let credential_pool = Arc::new(VertexCredentialPool::default());
        let ai_studio = Arc::new(GeminiAiStudioProvider::new(key_actor_handle.clone()));
        let vertex = Arc::new(GeminiVertexProvider::new(key_actor_handle, credential_pool));
        Self { ai_studio, vertex }
    }

    pub fn ai_studio(&self) -> Arc<GeminiAiStudioProvider> {
        self.ai_studio.clone()
    }

    pub fn vertex(&self) -> Arc<GeminiVertexProvider> {
        self.vertex.clone()
    }
}

pub struct GeminiAiStudioProvider {
    key_actor_handle: KeyActorHandle,
}

impl GeminiAiStudioProvider {
    fn new(key_actor_handle: KeyActorHandle) -> Self {
        Self { key_actor_handle }
    }

    fn build_state(&self, ctx: &GeminiContext) -> GeminiState {
        let mut state = GeminiState::new(self.key_actor_handle.clone());
        state.update_from_ctx(ctx);
        state
    }
}

#[async_trait::async_trait]
impl LLMProvider for GeminiAiStudioProvider {
    type Request = GeminiInvocation;
    type Output = Response;

    async fn invoke(&self, request: Self::Request) -> Result<Self::Output, ClewdrError> {
        if request.context.vertex {
            return Err(ClewdrError::BadRequest {
                msg: "Vertex request routed to AI Studio provider",
            });
        }
        log_request(&request.context);
        let mut state = self.build_state(&request.context);
        match request.payload {
            GeminiPayload::Native(body) => {
                if !request.context.stream {
                    let stream = keep_alive_stream(state, body);
                    return Response::builder()
                        .header("Content-Type", "application/json")
                        .body(Body::from_stream(stream))
                        .map_err(|e| ClewdrError::HttpError {
                            loc: Location::generate(),
                            source: e,
                        });
                }
                state.try_chat(body).await
            }
            GeminiPayload::OpenAI(body) => state.try_chat(body).await,
        }
    }
}

pub struct GeminiVertexProvider {
    key_actor_handle: KeyActorHandle,
    credentials: Arc<VertexCredentialPool>,
}

impl GeminiVertexProvider {
    fn new(key_actor_handle: KeyActorHandle, credentials: Arc<VertexCredentialPool>) -> Self {
        Self {
            key_actor_handle,
            credentials,
        }
    }

    fn build_state(&self, ctx: &GeminiContext) -> Result<GeminiState, ClewdrError> {
        let mut state = GeminiState::new(self.key_actor_handle.clone());
        state.update_from_ctx(ctx);
        let Some(credential) = self.credentials.next() else {
            return Err(ClewdrError::BadRequest {
                msg: "Vertex credential not found",
            });
        };
        state.vertex_credential = Some(credential);
        Ok(state)
    }
}

#[async_trait::async_trait]
impl LLMProvider for GeminiVertexProvider {
    type Request = GeminiInvocation;
    type Output = Response;

    async fn invoke(&self, request: Self::Request) -> Result<Self::Output, ClewdrError> {
        if !request.context.vertex {
            return Err(ClewdrError::BadRequest {
                msg: "AI Studio request routed to Vertex provider",
            });
        }
        log_request(&request.context);
        let mut state = self.build_state(&request.context)?;
        match request.payload {
            GeminiPayload::Native(body) => {
                if !request.context.stream {
                    let stream = keep_alive_stream(state, body);
                    return Response::builder()
                        .header("Content-Type", "application/json")
                        .body(Body::from_stream(stream))
                        .map_err(|e| ClewdrError::HttpError {
                            loc: Location::generate(),
                            source: e,
                        });
                }
                state.try_chat(body).await
            }
            GeminiPayload::OpenAI(body) => state.try_chat(body).await,
        }
    }
}

#[derive(Default)]
struct VertexCredentialPool {
    cursor: AtomicUsize,
}

impl VertexCredentialPool {
    fn next(&self) -> Option<ServiceAccountKey> {
        let creds = CLEWDR_CONFIG.load().vertex.credential_list();
        if creds.is_empty() {
            return None;
        }
        let idx = self.cursor.fetch_add(1, Ordering::Relaxed);
        Some(creds[idx % creds.len()].clone())
    }
}

fn log_request(ctx: &GeminiContext) {
    let format_label = if ctx.api_format == GeminiApiFormat::Gemini {
        ctx.api_format.to_string().green()
    } else {
        ctx.api_format.to_string().yellow()
    };
    info!(
        "[REQ] stream: {}, vertex: {}, format: {}, model: {}",
        enabled(ctx.stream),
        enabled(ctx.vertex),
        format_label,
        ctx.model.green()
    );
}

fn keep_alive_stream<T>(
    mut state: GeminiState,
    body: T,
) -> impl Stream<Item = Result<Bytes, axum::Error>>
where
    T: serde::Serialize + Clone + Send + 'static,
{
    let mut interval = tokio::time::interval(Duration::from_secs(15));
    let timeout = Duration::from_secs(360);
    stream! {
        let future = async move {
            state
                .try_chat(body.clone())
                .await
                .unwrap_or_else(|e| e.into_response())
                .into_body()
                .into_data_stream()
        };
        let stream = future.into_stream().flatten();
        pin_mut!(stream);
        let start = tokio::time::Instant::now();
        loop {
            select! {
                biased;
                data = stream.next() => {
                    match data {
                        Some(Ok(chunk)) => yield Ok(chunk),
                        Some(Err(err)) => {
                            yield Err(err);
                            break;
                        }
                        None => break,
                    }
                }
                _ = interval.tick() => {
                    if start.elapsed() > timeout {
                        break;
                    }
                    yield Ok(Bytes::from("\n"));
                }
                else => break,
            }
        }
    }
}
