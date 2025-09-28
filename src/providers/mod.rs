use async_trait::async_trait;

use crate::error::ClewdrError;

pub mod claude;
pub mod gemini;

#[async_trait]
pub trait LLMProvider: Send + Sync {
    type Request: Send;
    type Output: Send;

    async fn invoke(&self, request: Self::Request) -> Result<Self::Output, ClewdrError>;
}
