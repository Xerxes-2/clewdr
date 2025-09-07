use async_stream::stream;
use axum::{
    body::Body,
    extract::State,
    response::Response,
    Json,
};
use bytes::Bytes;
use colored::Colorize;
use futures::StreamExt;
use http::header::{CACHE_CONTROL, CONNECTION, CONTENT_TYPE};
use serde::Serialize;
use serde_json::json;
use tracing::info;

use crate::{
    error::ClewdrError,
    gemini_state::{GeminiApiFormat, GeminiState},
    middleware::gemini::{GeminiContext, GeminiOaiPreprocess, GeminiPreprocess},
    types::{gemini::request::{GeminiRequestBody, SystemInstruction}, oai::CreateMessageParams},
};
use crate::api::gemini::handle_gemini_request;

const DONE_MARKER: &str = "[done]";
const CONTINUATION_PROMPT: &str = "请从刚才被截断的地方继续输出剩余的所有内容。\n不要重复前面已经输出的内容。\n当你完整完成所有内容输出后，必须在最后一行单独输出：[done]";
const MAX_ATTEMPTS: usize = 3;

fn add_done_instruction_gemini(body: &mut GeminiRequestBody) {
    let instruction = format!(
        "请确保回答完整无遗漏，且在最后一行单独输出：{}。如果回答被中途打断，请在后续继续补充至完整再输出该标记。",
        DONE_MARKER
    );
    // For simplicity, override or set a system instruction
    body.system_instruction = Some(SystemInstruction::from_string(instruction));
}

fn add_continue_turn_gemini(body: &mut GeminiRequestBody) {
    // Fallback: refresh instruction to explicitly continue from truncation point
    body.system_instruction = Some(SystemInstruction::from_string(CONTINUATION_PROMPT));
}

fn add_done_instruction_oai(body: &mut CreateMessageParams) {
    use crate::types::claude::{Message as CMessage, Role as CRole};
    let sys = CMessage::new_text(
        CRole::System,
        format!(
            "请确保回答完整无遗漏，且在最后一行单独输出：{}。如果回答被中途打断，请在后续继续补充至完整再输出该标记。",
            DONE_MARKER
        ),
    );
    body.messages.insert(0, sys);
}

fn add_continue_turn_oai(body: &mut CreateMessageParams) {
    use crate::types::claude::{Message as CMessage, Role as CRole};
    let user = CMessage::new_text(CRole::User, CONTINUATION_PROMPT);
    body.messages.push(user);
}

fn stream_remove_marker(chunk: Bytes) -> (Bytes, bool) {
    let mut data = chunk.to_vec();
    let mut seen = false;
    let marker = DONE_MARKER.as_bytes();
    let mut i = 0usize;
    while i + marker.len() <= data.len() {
        if &data[i..i + marker.len()] == marker {
            // remove marker
            data.drain(i..i + marker.len());
            seen = true;
        } else {
            i += 1;
        }
    }
    (Bytes::from(data), seen)
}

async fn stream_with_anti_truncation<T>(
    state: GeminiState,
    body: T,
    mut prepare_first: impl FnMut(&mut T) + Send + 'static,
    mut prepare_next: impl FnMut(&mut T) + Send + 'static,
) -> Result<Response, ClewdrError>
where
    T: Serialize + Clone + Send + 'static,
{
    // Build a stream that may issue multiple upstream requests sequentially until DONE_MARKER is observed
    let s = stream! {
        let mut attempt = 0usize;
        let mut finished = false;
        while attempt < MAX_ATTEMPTS && !finished {
            let mut st = state.clone();
            let mut req_body = body.clone();
            if attempt == 0 { prepare_first(&mut req_body); } else { prepare_next(&mut req_body); }

            // Try streaming upstream; if upstream is not SSE, fallback to fake streaming
            st.stream = true;
            let resp = match st.send_chat(req_body.clone()).await {
                Ok(r) => r,
                Err(e) => { yield Err(axum::Error::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))); break; }
            };
            let is_sse = resp
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.starts_with("text/event-stream"))
                .unwrap_or(false);

            if !is_sse {
                // Fake streaming: poll non-stream response with heartbeats
                let mut st2 = state.clone();
                st2.stream = false;
                // adjust path for non-stream if possible
                if st2.path.contains("streamGenerateContent") {
                    st2.path = st2.path.replace("streamGenerateContent", "generateContent");
                }
                let req2 = body.clone();
                let handle = tokio::spawn(async move {
                    st2.send_chat(req2).await
                });
                match handle.await {
                    Ok(Ok(r)) => {
                        match r.bytes().await {
                            Ok(b) => {
                                let payload = format!("data: {}\n\n", String::from_utf8_lossy(&b));
                                yield Ok(Bytes::from(payload));
                            }
                            Err(e) => {
                                yield Err(axum::Error::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        yield Err(axum::Error::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                    }
                    Err(e) => {
                        yield Err(axum::Error::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
                    }
                }
                finished = true;
            } else {
                // Forward SSE while removing DONE_MARKER and detecting completion
                let mut stream = resp.bytes_stream();
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(chunk) => {
                            let (chunk, seen) = stream_remove_marker(chunk);
                            if seen { finished = true; }
                            yield Ok(chunk);
                        }
                        Err(e) => {
                            yield Err(axum::Error::new(e));
                            break;
                        }
                    }
                }
            }

            attempt += 1;
        }
    };

    let res = Response::builder()
        .header(CONTENT_TYPE, "text/event-stream")
        .header(CACHE_CONTROL, "no-cache")
        .header(CONNECTION, "keep-alive")
        .body(Body::from_stream(s))?;
    Ok(res)
}

async fn handle_cli_request<T: Serialize + Clone + Send + 'static>(
    mut state: GeminiState,
    body: T,
    ctx: GeminiContext,
    prepare_first: impl FnMut(&mut T) + Send + 'static,
    prepare_next: impl FnMut(&mut T) + Send + 'static,
) -> Result<Response, ClewdrError> {
    state.update_from_ctx(&ctx);
    info!(
        "[CLI] stream: {}, vertex: {}, format: {}, model: {}",
        crate::utils::enabled(ctx.stream),
        crate::utils::enabled(ctx.vertex),
        if ctx.api_format == GeminiApiFormat::Gemini {
            ctx.api_format.to_string().green()
        } else {
            ctx.api_format.to_string().yellow()
        },
        ctx.model.green(),
    );

    if ctx.stream {
        // Anti-truncation streaming with optional fake streaming fallback
        return stream_with_anti_truncation(state, body, prepare_first, prepare_next).await;
    }

    // Non-streaming: delegate to default handler with keep-alive
    handle_gemini_request(state, body, ctx).await
}

pub async fn api_post_gemini_cli(
    State(state): State<GeminiState>,
    GeminiPreprocess(body, ctx): GeminiPreprocess,
) -> Result<Response, ClewdrError> {
    handle_cli_request(
        state,
        body,
        ctx,
        |b: &mut GeminiRequestBody| add_done_instruction_gemini(b),
        |b: &mut GeminiRequestBody| add_continue_turn_gemini(b),
    )
    .await
}

pub async fn api_post_gemini_cli_oai(
    State(state): State<GeminiState>,
    GeminiOaiPreprocess(body, ctx): GeminiOaiPreprocess,
) -> Result<Response, ClewdrError> {
    handle_cli_request(
        state,
        body,
        ctx,
        |b: &mut CreateMessageParams| add_done_instruction_oai(b),
        |b: &mut CreateMessageParams| add_continue_turn_oai(b),
    )
    .await
}

// --- Models listing for CLI (Gemini native style) ---

#[allow(dead_code)]
fn gemini_cli_model_info(name: &str) -> serde_json::Value {
    let base = name.to_string();
    json!({
        "name": format!("models/{}", name),
        "baseModelId": base,
        "version": "001",
        "displayName": name,
        "description": format!("Gemini {} model", name),
        "inputTokenLimit": 128000,
        "outputTokenLimit": 8192,
        "supportedGenerationMethods": ["generateContent", "streamGenerateContent"],
        "temperature": 1.0,
        "maxTemperature": 2.0,
        "topP": 0.95,
        "topK": 64
    })
}

pub async fn api_gemini_cli_models() -> Result<Json<serde_json::Value>, ClewdrError> {
    // Provide a reasonable default list
    let models = [
        "gemini-1.5-flash",
        "gemini-1.5-pro",
        "gemini-1.0-pro",
    ];
    let items: Vec<_> = models.iter().map(|m| gemini_cli_model_info(m)).collect();
    Ok(Json(json!({ "models": items })))
}

use axum::extract::Path;
pub async fn api_gemini_cli_model_info(Path(path): Path<String>) -> Result<Json<serde_json::Value>, ClewdrError> {
    // Accept either `models/<id>` or `<id>`
    let id = path.strip_prefix("models/").unwrap_or(path.as_str());
    Ok(Json(gemini_cli_model_info(id)))
}
