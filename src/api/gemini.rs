use axum::{extract::State, response::Response};

use crate::{
    error::ClewdrError,
    middleware::gemini::{GeminiOaiPreprocess, GeminiPreprocess},
    providers::{
        LLMProvider,
        gemini::{GeminiInvocation, GeminiPayload, GeminiProviders},
    },
};

pub async fn api_post_gemini(
    State(providers): State<GeminiProviders>,
    GeminiPreprocess(body, ctx): GeminiPreprocess,
) -> Result<Response, ClewdrError> {
    if ctx.vertex {
        providers
            .vertex()
            .invoke(GeminiInvocation {
                payload: GeminiPayload::Native(body),
                context: ctx,
            })
            .await
    } else {
        providers
            .ai_studio()
            .invoke(GeminiInvocation {
                payload: GeminiPayload::Native(body),
                context: ctx,
            })
            .await
    }
}

pub async fn api_post_gemini_oai(
    State(providers): State<GeminiProviders>,
    GeminiOaiPreprocess(body, ctx): GeminiOaiPreprocess,
) -> Result<Response, ClewdrError> {
    if ctx.vertex {
        providers
            .vertex()
            .invoke(GeminiInvocation {
                payload: GeminiPayload::OpenAI(body),
                context: ctx,
            })
            .await
    } else {
        providers
            .ai_studio()
            .invoke(GeminiInvocation {
                payload: GeminiPayload::OpenAI(body),
                context: ctx,
            })
            .await
    }
}
