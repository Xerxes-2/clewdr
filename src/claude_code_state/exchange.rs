use std::{collections::HashMap, pin::Pin, str::FromStr};

use http::header::{COOKIE, USER_AGENT};
use oauth2::{
    AsyncHttpClient, AuthUrl, AuthorizationCode, Client, ClientId, CsrfToken, EndpointNotSet,
    EndpointSet, HttpClientError, HttpRequest, HttpResponse, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, StandardRevocableToken, TokenUrl,
    basic::{
        BasicErrorResponse, BasicRevocationErrorResponse, BasicTokenIntrospectionResponse,
        BasicTokenResponse,
    },
    http::{
        self,
        header::{HeaderName, HeaderValue},
    },
};
use serde_json::Value;
use snafu::{OptionExt, ResultExt};
use url::Url;

use crate::{
    claude_code_state::ClaudeCodeState,
    config::{CC_REDIRECT_URI, CC_TOKEN_URL, CLEWDR_CONFIG, CookieStatus, TokenInfo},
    error::{CheckClaudeErr, ClewdrError, UnexpectedNoneSnafu, UrlSnafu, WreqSnafu},
};

use super::chat::{CLAUDE_API_VERSION, CLAUDE_BETA_BASE};

const CLAUDE_CODE_USER_AGENT: &str = "claude-code/2.0.32";

type ClaudeOauthClient = Client<
    BasicErrorResponse,
    BasicTokenResponse,
    BasicTokenIntrospectionResponse,
    StandardRevocableToken,
    BasicRevocationErrorResponse,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointSet,
>;

struct OauthClient {
    client: wreq::Client,
}

impl<'c> AsyncHttpClient<'c> for OauthClient {
    type Error = HttpClientError<wreq::Error>;

    type Future = Pin<Box<dyn Future<Output = Result<HttpResponse, Self::Error>> + Send + 'c>>;

    fn call(&'c self, mut request: HttpRequest) -> Self::Future {
        {
            let headers = request.headers_mut();
            headers.insert(
                HeaderName::from_static("anthropic-version"),
                HeaderValue::from_static(CLAUDE_API_VERSION),
            );
            headers.insert(
                HeaderName::from_static("anthropic-beta"),
                HeaderValue::from_static(CLAUDE_BETA_BASE),
            );
        }

        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let mut req = self.client.request(parts.method, parts.uri.to_string());
            for (name, value) in &parts.headers {
                req = req.header(name, value.clone());
            }
            let response = req.body(body).send().await.map_err(Box::new)?;

            let mut builder = http::Response::builder().status(response.status());

            {
                builder = builder.version(response.version());
            }

            for (name, value) in response.headers().iter() {
                builder = builder.header(name, value);
            }

            builder
                .body(response.bytes().await.map_err(Box::new)?.to_vec())
                .map_err(HttpClientError::Http)
        })
    }
}

pub struct ExchangeResult {
    code: String,
    state: Option<String>,
    verifier: PkceCodeVerifier,
    org_uuid: String,
}

fn setup_client(cc_client_id: String) -> Result<ClaudeOauthClient, ClewdrError> {
    Ok(oauth2::basic::BasicClient::new(ClientId::new(cc_client_id))
        .set_auth_type(oauth2::AuthType::RequestBody)
        .set_redirect_uri(RedirectUrl::new(CC_REDIRECT_URI.into()).map_err(|_| {
            ClewdrError::UnexpectedNone {
                msg: "Invalid redirect URI",
            }
        })?)
        .set_token_uri(TokenUrl::new(CC_TOKEN_URL.into()).map_err(|_| {
            ClewdrError::UnexpectedNone {
                msg: "Invalid token URI",
            }
        })?))
}

impl ClaudeCodeState {
    pub async fn exchange_code(&self, org_uuid: &str) -> Result<ExchangeResult, ClewdrError> {
        // Build OAuth authorization URL using Url::join for proper URL construction
        let authorize_url = CLEWDR_CONFIG
            .load()
            .endpoint()
            .join(&format!("v1/oauth/{}/authorize", org_uuid))
            .expect("Url parse error");
        let cc_client_id = CLEWDR_CONFIG.load().cc_client_id();

        let client = setup_client(cc_client_id)?.set_auth_uri(
            AuthUrl::from_url(authorize_url), // Avoid reparsing the URL
        );

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let (mut auth_url, _csrf_token) = client
            .authorize_url(|| CsrfToken::new_random_len(32))
            .add_scope(Scope::new("user:profile".to_string()))
            .add_scope(Scope::new("user:inference".to_string()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        let mut query_params: HashMap<String, String> =
            auth_url.query_pairs().into_owned().collect();
        query_params.insert("organization_uuid".to_string(), org_uuid.to_string());
        auth_url.set_query(None);

        let wreq_client = self.get_wreq_client();
        let mut authorize_req = wreq_client
            .post(auth_url.to_string())
            .header(USER_AGENT, CLAUDE_CODE_USER_AGENT)
            .json(&query_params);
        if let Some(cookie) = self.cookie.as_ref() {
            authorize_req = authorize_req.header(COOKIE, cookie.cookie.to_string());
        }
        let redirect_json = authorize_req
            .send()
            .await
            .context(WreqSnafu {
                msg: "Failed to send authorization request",
            })?
            .check_claude()
            .await?
            .json::<Value>()
            .await
            .context(WreqSnafu {
                msg: "Failed to parse authorization response",
            })?;

        let redirect_uri = redirect_json["redirect_uri"]
            .as_str()
            .expect("Expected redirect_uri in response");
        let redirect_url = Url::from_str(redirect_uri).context(UrlSnafu {
            url: redirect_uri.to_string(),
        })?;

        let query = redirect_url.query_pairs().collect::<HashMap<_, _>>();
        let code = query.get("code").context(UnexpectedNoneSnafu {
            msg: "No code found in redirect URL",
        })?;
        let state = query.get("state");

        Ok(ExchangeResult {
            code: code.to_string(),
            state: state.map(|s| s.to_string()),
            verifier: pkce_verifier,
            org_uuid: org_uuid.to_string(),
        })
    }

    pub async fn exchange_token(&mut self, code_res: ExchangeResult) -> Result<(), ClewdrError> {
        let cc_client_id = CLEWDR_CONFIG.load().cc_client_id();

        let client = setup_client(cc_client_id)?;

        let wreq_client = self.get_wreq_client();
        let my_client = OauthClient {
            client: wreq_client.clone(),
        };

        let mut token_request = client
            .exchange_code(AuthorizationCode::new(code_res.code))
            .set_pkce_verifier(code_res.verifier);

        if let Some(state) = code_res.state {
            token_request = token_request.add_extra_param("state", state);
        }

        let token = token_request.request_async(&my_client).await?;

        if let Some(cookie) = self.cookie.as_mut() {
            cookie.token = Some(TokenInfo::new(token, code_res.org_uuid.clone()));
        } else {
            return Err(ClewdrError::UnexpectedNone {
                msg: "No cookie found to update with token info",
            });
        }
        Ok(())
    }

    pub async fn refresh_token(&mut self) -> Result<(), ClewdrError> {
        let wreq_client = self.get_wreq_client();
        let Some(CookieStatus {
            token: Some(ref mut token),
            ..
        }) = self.cookie
        else {
            return Err(ClewdrError::UnexpectedNone {
                msg: "No token found to refresh token",
            });
        };
        if !token.is_expired() {
            return Ok(());
        }

        let cc_client_id = CLEWDR_CONFIG.load().cc_client_id();

        let client = oauth2::basic::BasicClient::new(ClientId::new(cc_client_id))
            .set_auth_type(oauth2::AuthType::RequestBody)
            .set_token_uri(TokenUrl::new(CC_TOKEN_URL.into()).map_err(|_| {
                ClewdrError::UnexpectedNone {
                    msg: "Invalid token URI",
                }
            })?);

        let my_client = OauthClient {
            client: wreq_client.clone(),
        };

        let org_uuid = token.organization.uuid.clone();
        let refresh_result = client
            .exchange_refresh_token(&oauth2::RefreshToken::new(token.refresh_token.to_owned()))
            .request_async(&my_client)
            .await;

        match refresh_result {
            Ok(new_token) => {
                *token = TokenInfo::new(new_token, org_uuid);
                Ok(())
            }
            Err(e) => {
                // Check if this is an invalid_grant error
                if !Self::is_invalid_grant_error(&e) {
                    return Err(e.into());
                }
                tracing::warn!(
                    "Refresh token invalid (invalid_grant), attempting to re-authorize with new OAuth2 flow"
                );
                // Clear the old token to force re-authorization
                if let Some(cookie) = self.cookie.as_mut() {
                    cookie.token = None;
                }

                // First, verify the cookie is still valid and check account type
                // This will return Reason::Null if cookie is invalid,
                // or Reason::Free if account was downgraded
                let org_uuid = self
                    .get_organization()
                    .await
                    .inspect_err(|e| tracing::error!("Cannot re-authorize: {}", e))?;

                // Cookie is valid and account has Pro+ permissions, proceed with re-authorization
                let code_res = self.exchange_code(&org_uuid).await.inspect_err(|e| {
                    tracing::error!("Failed to exchange code during re-authorization: {}", e)
                })?;
                match self.exchange_token(code_res).await {
                    Ok(_) => {
                        tracing::info!("Successfully re-authorized with new OAuth2 flow");
                        Ok(())
                    }
                    Err(token_err) => {
                        tracing::error!(
                            "Failed to exchange token during re-authorization: {}",
                            token_err
                        );
                        Err(token_err)
                    }
                }
            }
        }
    }

    /// Checks if the error is an invalid_grant error from OAuth2
    fn is_invalid_grant_error(
        error: &oauth2::RequestTokenError<
            oauth2::HttpClientError<wreq::Error>,
            oauth2::StandardErrorResponse<oauth2::basic::BasicErrorResponseType>,
        >,
    ) -> bool {
        use oauth2::RequestTokenError;
        match error {
            RequestTokenError::ServerResponse(response) => {
                // Check if error type is invalid_grant
                response
                    .error()
                    .to_string()
                    .to_lowercase()
                    .contains("invalid_grant")
                    || response
                        .error_description()
                        .map(|desc| {
                            let desc_lower = desc.to_lowercase();
                            desc_lower.contains("refresh token not found")
                                || desc_lower.contains("refresh token")
                                    && desc_lower.contains("invalid")
                        })
                        .unwrap_or(false)
            }
            _ => false,
        }
    }

    fn get_wreq_client(&self) -> wreq::Client {
        self.client.clone()
    }
}
