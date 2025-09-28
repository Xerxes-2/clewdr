use axum::{
    Json, RequestExt,
    extract::{FromRequest, Path, Request},
};

use super::GeminiArgs;
use crate::{
    config::CLEWDR_CONFIG,
    error::ClewdrError,
    gemini_state::GeminiApiFormat,
    types::{gemini::request::GeminiRequestBody, oai::CreateMessageParams},
};

#[derive(Clone)]
pub struct GeminiContext {
    pub model: String,
    pub vertex: bool,
    pub stream: bool,
    pub path: String,
    pub query: GeminiArgs,
    pub api_format: GeminiApiFormat,
}

pub struct GeminiPreprocess(pub GeminiRequestBody, pub GeminiContext);

impl<S> FromRequest<S> for GeminiPreprocess
where
    S: Send + Sync,
{
    type Rejection = ClewdrError;

    async fn from_request(mut req: Request, _: &S) -> Result<Self, Self::Rejection> {
        let Path(path) = req.extract_parts::<Path<String>>().await?;
        let vertex = req.uri().to_string().contains("vertex");
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
        let ctx = GeminiContext {
            vertex,
            model,
            stream: path.contains("streamGenerateContent"),
            path,
            query,
            api_format: GeminiApiFormat::Gemini,
        };
        let Json(mut body) = Json::<GeminiRequestBody>::from_request(req, &()).await?;
        body.safety_off();
        Ok(GeminiPreprocess(body, ctx))
    }
}

pub struct GeminiOaiPreprocess(pub CreateMessageParams, pub GeminiContext);

impl<S> FromRequest<S> for GeminiOaiPreprocess
where
    S: Send + Sync,
{
    type Rejection = ClewdrError;

    async fn from_request(req: Request, _: &S) -> Result<Self, Self::Rejection> {
        let vertex = req.uri().to_string().contains("vertex");
        if vertex && !CLEWDR_CONFIG.load().vertex.validate() {
            return Err(ClewdrError::BadRequest {
                msg: "Vertex is not configured",
            });
        }
        let Json(mut body) = Json::<CreateMessageParams>::from_request(req, &()).await?;
        let model = body.model.to_owned();
        if vertex {
            body.preprocess_vertex();
        }
        let stream = body.stream.unwrap_or_default();
        let ctx = GeminiContext {
            vertex,
            model,
            stream,
            path: String::new(),
            query: GeminiArgs::default(),
            api_format: GeminiApiFormat::OpenAI,
        };
        Ok(GeminiOaiPreprocess(body, ctx))
    }
}
