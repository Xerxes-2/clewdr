use async_stream::stream;
use axum::{
    body::Body,
    extract::State,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use colored::Colorize;
use futures::{FutureExt, Stream, StreamExt, pin_mut};
use http::header::CONTENT_TYPE;
use serde::Serialize;
use tokio::select;
use tracing::info;

use crate::{
    error::ClewdrError,
    gemini_state::{GeminiApiFormat, GeminiState},
    middleware::gemini::{GeminiContext, GeminiOaiPreprocess, GeminiPreprocess},
    utils::enabled,
};

// Common handler function to process both Gemini and OpenAI format requests
pub async fn handle_gemini_request<T: Serialize + Clone + Send + 'static>(
    mut state: GeminiState,
    body: T,
    ctx: GeminiContext,
) -> Result<Response, ClewdrError> {
    state.update_from_ctx(&ctx);
    let GeminiContext {
        model,
        stream,
        vertex,
        ..
    } = ctx;
    info!(
        "[REQ] stream: {}, vertex: {}, format: {}, model: {}",
        enabled(stream),
        enabled(vertex),
        if ctx.api_format == GeminiApiFormat::Gemini {
            ctx.api_format.to_string().green()
        } else {
            ctx.api_format.to_string().yellow()
        },
        model.green(),
    );

    // For non-streaming requests, we need to handle keep-alive differently
    if !stream {
        let stream = keep_alive_stream(state, body);
        let res = Response::builder()
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from_stream(stream))?;
        return Ok(res);
    }

    // For streaming requests, proceed as before
    let res = state.try_chat(body).await?;
    Ok(res)
}

fn keep_alive_stream<T>(
    mut state: GeminiState,
    body: T,
) -> impl Stream<Item = Result<Bytes, axum::Error>>
where
    T: Serialize + Clone + Send + 'static,
{
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
    let time_out = std::time::Duration::from_secs(360);
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
        let start = std::time::Instant::now();
        loop {
            select! {
                biased;
                data = stream.next() => {
                    match data {
                        Some(Ok(d)) => yield Ok(d),
                        Some(Err(e)) => {
                            yield Err(e);
                            break;
                        }
                        None => break
                    }
                }
                _ = interval.tick() => {
                    if start.elapsed() > time_out {
                        break;
                    }
                    yield Ok(Bytes::from("\n"));
                }
                else => break
            }
        }
    }
}

pub async fn api_post_gemini(
    State(state): State<GeminiState>,
    GeminiPreprocess(body, ctx): GeminiPreprocess,
) -> Result<Response, ClewdrError> {
    handle_gemini_request(state, body, ctx).await
}

pub async fn api_post_gemini_oai(
    State(state): State<GeminiState>,
    GeminiOaiPreprocess(body, ctx): GeminiOaiPreprocess,
) -> Result<Response, ClewdrError> {
    handle_gemini_request(state, body, ctx).await
}
