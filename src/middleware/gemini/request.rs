use axum::{
    Json, RequestExt,
    extract::{FromRequest, Path, Request},
};

use super::GeminiArgs;
use crate::{
    config::CLEWDR_CONFIG,
    error::ClewdrError,
    gemini_state::{GeminiApiFormat, GeminiState},
    types::{gemini::request::GeminiRequestBody, oai::CreateMessageParams},
};

pub struct GeminiContext {
    pub model: String,
    pub vertex: bool,
    pub stream: bool,
    pub path: String,
    pub query: GeminiArgs,
    pub api_format: GeminiApiFormat,
    pub cli_mode: bool,
    pub auth_bearer: Option<String>,
}

pub struct GeminiPreprocess(pub GeminiRequestBody, pub GeminiContext);

impl FromRequest<GeminiState> for GeminiPreprocess {
    type Rejection = ClewdrError;

    async fn from_request(mut req: Request, state: &GeminiState) -> Result<Self, Self::Rejection> {
        let Path(path) = req.extract_parts::<Path<String>>().await?;
        let uri = req.uri().to_string();
        let vertex = uri.contains("vertex");
        let cli_mode = uri.contains("/gemini/cli/");
        if vertex && !CLEWDR_CONFIG.load().vertex.validate() {
            return Err(ClewdrError::BadRequest {
                msg: "Vertex is not configured",
            });
        }
        let mut model = path
            .split('/')
            .next_back()
            .map(|s| s.split_once(':').map(|s| s.0).unwrap_or(s).to_string());
        if vertex {
            model = CLEWDR_CONFIG.load().vertex.model_id.to_owned().or(model)
        }
        let Some(model) = model else {
            return Err(ClewdrError::BadRequest {
                msg: "Model not found in path or vertex config",
            });
        };
        let query = req.extract_parts::<GeminiArgs>().await?;
        // extract Authorization bearer if present
        let auth_bearer = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.to_string());
        let ctx = GeminiContext {
            vertex,
            model,
            stream: path.contains("streamGenerateContent"),
            path,
            query,
            api_format: GeminiApiFormat::Gemini,
            cli_mode,
            auth_bearer,
        };
        let Json(mut body) = Json::<GeminiRequestBody>::from_request(req, &()).await?;
        body.safety_off();
        let mut state = state.clone();
        state.update_from_ctx(&ctx);
        Ok(GeminiPreprocess(body, ctx))
    }
}

pub struct GeminiOaiPreprocess(pub CreateMessageParams, pub GeminiContext);

impl FromRequest<GeminiState> for GeminiOaiPreprocess {
    type Rejection = ClewdrError;

    async fn from_request(req: Request, state: &GeminiState) -> Result<Self, Self::Rejection> {
        let uri = req.uri().to_string();
        let vertex = uri.contains("vertex");
        let cli_mode = uri.contains("/gemini/cli/");
        if vertex && !CLEWDR_CONFIG.load().vertex.validate() {
            return Err(ClewdrError::BadRequest {
                msg: "Vertex is not configured",
            });
        }
        // capture bearer before request is consumed
        let auth_bearer = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.to_string());
        let Json(mut body) = Json::<CreateMessageParams>::from_request(req, &()).await?;
        let model = body.model.to_owned();
        if vertex {
            body.preprocess_vertex();
        }
        let stream = body.stream.unwrap_or_default();
        // auth_bearer captured above
        let ctx = GeminiContext {
            vertex,
            model,
            stream,
            path: String::new(),
            query: GeminiArgs::default(),
            api_format: GeminiApiFormat::OpenAI,
            cli_mode,
            auth_bearer,
        };
        let mut state = state.clone();
        state.update_from_ctx(&ctx);
        Ok(GeminiOaiPreprocess(body, ctx))
    }
}
