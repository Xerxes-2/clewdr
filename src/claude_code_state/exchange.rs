use std::{collections::HashMap, str::FromStr};

use http::header::{ACCEPT, CONTENT_TYPE, COOKIE, USER_AGENT};
use serde_json::Value;
use snafu::{OptionExt, ResultExt};
use url::Url;

use super::{
    chat::{CLAUDE_API_VERSION, CLAUDE_BETA_BASE},
    oauth::{
        Pkce, TokenErrorResponse, TokenResponse, authorization_code_form, authorize_params,
        random_state, refresh_token_form,
    },
};
use crate::{
    claude_code_state::ClaudeCodeState,
    config::{
        CC_REDIRECT_URI, CC_TOKEN_URL, CLAUDE_CODE_USER_AGENT, CLEWDR_CONFIG, CookieStatus,
        TokenInfo,
    },
    error::{
        CheckClaudeErr, ClewdrError, RequestTokenSnafu, UnexpectedNoneSnafu, UrlSnafu, WreqSnafu,
    },
};

/// The scopes this client asks for.
const SCOPES: [&str; 2] = ["user:profile", "user:inference"];

pub struct ExchangeResult {
    code: String,
    state: Option<String>,
    pkce: Pkce,
    org_uuid: String,
}

/// The outcome of a token request that the caller has to tell apart.
///
/// A rejected grant is not treated as a failure by every caller: a refresh
/// that comes back `invalid_grant` is recoverable by authorizing again, so the
/// distinction is carried in the return type rather than buried in an error
/// string that would have to be matched on.
enum TokenOutcome {
    Granted(Box<TokenResponse>),
    Rejected(TokenErrorResponse),
}

impl ClaudeCodeState {
    /// Run the OAuth authorization step and return the code to exchange.
    ///
    /// # Errors
    /// Upstream HTTP failures, a response that carries no `redirect_uri`, or a
    /// redirect URL with no `code` parameter.
    ///
    /// # Panics
    /// If the configured endpoint cannot be joined with the authorize path,
    /// which would mean the endpoint itself is malformed.
    pub async fn exchange_code(&self, org_uuid: &str) -> Result<ExchangeResult, ClewdrError> {
        // Build OAuth authorization URL using Url::join for proper URL construction
        let authorize_url = CLEWDR_CONFIG
            .load()
            .endpoint()
            .join(&format!("v1/oauth/{org_uuid}/authorize"))
            .expect("Url parse error");
        let cc_client_id = CLEWDR_CONFIG.load().cc_client_id();

        let pkce = Pkce::new_random();
        let mut query_params = authorize_params(
            &cc_client_id,
            CC_REDIRECT_URI,
            &SCOPES,
            &pkce,
            &random_state(),
        );
        query_params.insert("organization_uuid".to_string(), org_uuid.to_string());

        let wreq_client = self.get_wreq_client();
        let mut authorize_req = wreq_client
            .post(authorize_url.to_string())
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

        let redirect_uri =
            redirect_json["redirect_uri"]
                .as_str()
                .ok_or_else(|| ClewdrError::Whatever {
                    message: "No reditect_uri found".to_string(),
                    source: None,
                })?;
        let parsed = Url::from_str(redirect_uri).context(UrlSnafu {
            url: redirect_uri.to_string(),
        })?;

        let query = parsed.query_pairs().collect::<HashMap<_, _>>();
        let code = query.get("code").context(UnexpectedNoneSnafu {
            msg: "No code found in redirect URL",
        })?;
        let state = query.get("state");

        Ok(ExchangeResult {
            code: code.to_string(),
            state: state.map(std::string::ToString::to_string),
            pkce,
            org_uuid: org_uuid.to_string(),
        })
    }

    /// Trade an authorization code for an access token and store it.
    ///
    /// # Errors
    /// Upstream HTTP failures, or a token response that cannot be parsed.
    pub async fn exchange_token(&mut self, code_res: ExchangeResult) -> Result<(), ClewdrError> {
        let cc_client_id = CLEWDR_CONFIG.load().cc_client_id();

        let form = authorization_code_form(
            &cc_client_id,
            CC_REDIRECT_URI,
            &code_res.code,
            &code_res.pkce,
            code_res.state.as_deref(),
        );

        let token = match self.post_token_request(form).await? {
            TokenOutcome::Granted(token) => token,
            TokenOutcome::Rejected(error) => {
                return RequestTokenSnafu {
                    msg: error.to_string(),
                }
                .fail();
            }
        };

        if let Some(cookie) = self.cookie.as_mut() {
            cookie.token = Some(TokenInfo::new(&token, code_res.org_uuid.clone()));
        } else {
            return Err(ClewdrError::UnexpectedNone {
                msg: "No cookie found to update with token info",
            });
        }
        Ok(())
    }

    /// Refresh an expired access token using the stored refresh token.
    ///
    /// # Errors
    /// Upstream HTTP failures, or a token response that cannot be parsed.
    pub async fn refresh_token(&mut self) -> Result<(), ClewdrError> {
        let Some(CookieStatus {
            token: Some(ref token),
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

        // Copied out so the borrow of self.cookie ends before the request;
        // the reply is written back through a fresh borrow below.
        let org_uuid = token.organization.uuid.clone();
        let form = refresh_token_form(&cc_client_id, &token.refresh_token);

        match self.post_token_request(form).await? {
            TokenOutcome::Granted(new_token) => {
                let Some(CookieStatus {
                    token: Some(ref mut token),
                    ..
                }) = self.cookie
                else {
                    return Err(ClewdrError::UnexpectedNone {
                        msg: "No token found to refresh token",
                    });
                };
                *token = TokenInfo::new(&new_token, org_uuid);
                Ok(())
            }
            TokenOutcome::Rejected(error) => {
                // Anything other than a rejected grant is a real failure;
                // re-authorizing would not help and would cost a round trip.
                if !error.is_invalid_grant() {
                    return RequestTokenSnafu {
                        msg: error.to_string(),
                    }
                    .fail();
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
                    tracing::error!("Failed to exchange code during re-authorization: {}", e);
                })?;
                match self.exchange_token(code_res).await {
                    Ok(()) => {
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

    /// Posts a form to the token endpoint and classifies the reply.
    ///
    /// # Errors
    /// Transport failures, and any reply that is neither a token nor a
    /// well-formed error -- an HTML error page from a proxy, say.
    async fn post_token_request(
        &self,
        form: Vec<(String, String)>,
    ) -> Result<TokenOutcome, ClewdrError> {
        let response = self
            .get_wreq_client()
            .post(CC_TOKEN_URL)
            .header(ACCEPT, "application/json")
            .header("anthropic-version", CLAUDE_API_VERSION)
            .header("anthropic-beta", CLAUDE_BETA_BASE)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(urlencode(&form))
            .send()
            .await
            .context(WreqSnafu {
                msg: "Failed to send token request",
            })?;

        let status = response.status();
        let body = response.bytes().await.context(WreqSnafu {
            msg: "Failed to read token response",
        })?;

        classify_token_response(status.as_u16(), &body)
    }

    fn get_wreq_client(&self) -> wreq::Client {
        self.client.clone()
    }
}

/// Reads a token endpoint reply.
///
/// Separated from the request so the classification can be tested against
/// recorded replies; it is the part with rules in it.
///
/// # Errors
/// A 200 whose body is not a token, or a non-200 whose body is not the
/// documented error shape -- an HTML page from a proxy in front of the API,
/// most likely.
fn classify_token_response(status: u16, body: &[u8]) -> Result<TokenOutcome, ClewdrError> {
    // Only 200 carries a token. RFC 6749 gives every other status to the error
    // path, and the previous implementation drew the line in the same place,
    // so a 201 is treated as a failure rather than guessed at.
    if status == 200 {
        return serde_json::from_slice(body)
            .map(|token| TokenOutcome::Granted(Box::new(token)))
            .map_err(|e| {
                RequestTokenSnafu {
                    msg: format!("malformed token response: {e}"),
                }
                .build()
            });
    }

    // A body that is not the documented error shape means something other than
    // the token endpoint answered. Reporting it beats mistaking it for a
    // rejected grant, which would start a re-authorization that fails the same
    // way and discards a refresh token that may still be good.
    serde_json::from_slice::<TokenErrorResponse>(body)
        .ok()
        .filter(|error| !error.error.is_empty() || error.error_description.is_some())
        .map(TokenOutcome::Rejected)
        .ok_or_else(|| {
            RequestTokenSnafu {
                msg: format!(
                    "token endpoint returned {status}: {}",
                    String::from_utf8_lossy(body)
                        .chars()
                        .take(200)
                        .collect::<String>()
                ),
            }
            .build()
        })
}

/// Encodes form pairs as `application/x-www-form-urlencoded`.
///
/// Done here rather than through wreq's `form` feature, which would pull in a
/// serialiser for the sake of a flat list of string pairs.
fn urlencode(pairs: &[(String, String)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recorded byte-for-byte from the `oauth2` crate this replaced. An
    /// authorization code is opaque and routinely contains `+`, `/` and `=`;
    /// sending those raw would corrupt the code and fail every login, and the
    /// failure would look like an upstream problem rather than an encoding
    /// bug.
    #[test]
    fn the_form_encoding_is_unchanged() {
        let form = vec![
            ("grant_type".to_owned(), "authorization_code".to_owned()),
            ("code".to_owned(), "THE CODE/+=".to_owned()),
            (
                "redirect_uri".to_owned(),
                "https://console.anthropic.com/oauth/code/callback".to_owned(),
            ),
        ];

        assert_eq!(
            urlencode(&form),
            "grant_type=authorization_code\
             &code=THE+CODE%2F%2B%3D\
             &redirect_uri=https%3A%2F%2Fconsole.anthropic.com%2Foauth%2Fcode%2Fcallback"
        );
    }

    /// Pairs go out in the order given, which is the order the previous
    /// implementation used.
    #[test]
    fn the_form_preserves_order() {
        let form = vec![
            ("b".to_owned(), "1".to_owned()),
            ("a".to_owned(), "2".to_owned()),
        ];

        assert_eq!(urlencode(&form), "b=1&a=2");
    }

    /// Byte-for-byte comparison against output recorded from the `oauth2`
    /// crate, with the random values held fixed. This is the check that the
    /// endpoint sees exactly what it saw before.
    #[test]
    fn the_request_bodies_match_the_previous_implementation_byte_for_byte() {
        const ID: &str = "CLIENT_ID_X";
        const REDIRECT: &str = "https://console.anthropic.com/oauth/code/callback";
        const VERIFIER: &str = "C9XYlGBnyeoIiuPZbwnMoY0TvXrofivIJfnFDfkoJN4";

        let mut form = super::super::oauth::authorization_code_form(
            ID,
            REDIRECT,
            "THE CODE/+=",
            &super::super::oauth::Pkce::new_random(),
            Some("THE STATE"),
        );
        // Substitute the recorded verifier for the generated one.
        for pair in &mut form {
            if pair.0 == "code_verifier" {
                pair.1 = VERIFIER.to_owned();
            }
        }

        assert_eq!(
            urlencode(&form),
            "grant_type=authorization_code&code=THE+CODE%2F%2B%3D\
             &code_verifier=C9XYlGBnyeoIiuPZbwnMoY0TvXrofivIJfnFDfkoJN4\
             &client_id=CLIENT_ID_X\
             &redirect_uri=https%3A%2F%2Fconsole.anthropic.com%2Foauth%2Fcode%2Fcallback\
             &state=THE+STATE"
        );

        assert_eq!(
            urlencode(&super::super::oauth::refresh_token_form(ID, "THE REFRESH")),
            "grant_type=refresh_token&refresh_token=THE+REFRESH&client_id=CLIENT_ID_X"
        );
    }

    fn granted(status: u16, body: &str) -> TokenResponse {
        match classify_token_response(status, body.as_bytes()) {
            Ok(TokenOutcome::Granted(token)) => *token,
            other => panic!("expected a token, got {:?}", other.map(|_| "rejected")),
        }
    }

    fn rejected(status: u16, body: &str) -> TokenErrorResponse {
        match classify_token_response(status, body.as_bytes()) {
            Ok(TokenOutcome::Rejected(error)) => error,
            other => panic!("expected a rejection, got {:?}", other.map(|_| "granted")),
        }
    }

    #[test]
    fn a_200_carrying_a_token_is_granted() {
        let token = granted(
            200,
            r#"{"access_token":"AT","token_type":"bearer","expires_in":3600,"refresh_token":"RT"}"#,
        );

        assert_eq!(token.access_token, "AT");
        assert_eq!(token.refresh_token.as_deref(), Some("RT"));
    }

    /// Matches the previous implementation, which accepted only 200. A 2xx
    /// that is not 200 is not a token response, and treating one as success
    /// would store an empty access token.
    #[test]
    fn a_non_200_success_is_not_a_token() {
        let body = r#"{"access_token":"AT","token_type":"bearer"}"#;

        for status in [201u16, 202, 204, 299] {
            assert!(
                classify_token_response(status, body.as_bytes()).is_err(),
                "status {status} should not be read as a token"
            );
        }
    }

    #[test]
    fn an_error_body_is_a_rejection() {
        let error = rejected(
            400,
            r#"{"error":"invalid_grant","error_description":"Refresh token not found"}"#,
        );

        assert!(error.is_invalid_grant());
    }

    /// A rejection is only recognised from the documented shape. An HTML
    /// error page from a proxy in front of the API used to deserialize into
    /// an all-default error, which reads as "not `invalid_grant`" and would be
    /// reported -- but an empty error with no description is indistinguishable
    /// from a successful parse, so it is rejected explicitly.
    #[test]
    fn a_body_that_is_not_an_error_response_is_an_error() {
        for body in [
            "<html>502 Bad Gateway</html>",
            "",
            "{}",
            r#"{"detail":"rate limited"}"#,
        ] {
            let result = classify_token_response(500, body.as_bytes());

            assert!(
                result.is_err(),
                "{body:?} should not be read as a rejected grant"
            );
        }
    }

    /// The `invalid_grant` path discards the refresh token and re-authorizes,
    /// so a garbled 200 must not reach it by way of a default-constructed
    /// error.
    #[test]
    fn a_malformed_token_response_is_an_error() {
        let result = classify_token_response(200, b"<html>hello</html>");

        assert!(result.is_err());
    }

    /// The endpoint's own error text is worth keeping: it is the only clue
    /// when something between here and Anthropic is answering instead.
    #[test]
    fn an_unrecognised_body_is_reported_with_its_status() {
        let Err(e) = classify_token_response(502, b"<html>Bad Gateway</html>") else {
            panic!("expected an error");
        };

        let msg = e.to_string();
        assert!(msg.contains("502"), "{msg}");
        assert!(msg.contains("Bad Gateway"), "{msg}");
    }

    /// A proxy can return a very long HTML page; the message goes into logs
    /// and an error response, so it is truncated.
    #[test]
    fn a_long_error_body_is_truncated() {
        let body = "x".repeat(10_000);

        let Err(e) = classify_token_response(500, body.as_bytes()) else {
            panic!("expected an error");
        };

        assert!(e.to_string().len() < 400, "{}", e.to_string().len());
    }
}
