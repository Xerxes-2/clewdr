//! The OAuth wire format used against Anthropic's token endpoint.
//!
//! Only the authorization-code grant with PKCE and the refresh grant are
//! implemented, which is all this flow uses. Everything the endpoint sees --
//! parameter names, encodings, and which status codes count as success -- is
//! decided here, and was matched against the `oauth2` crate's output before
//! replacing it. The tests below record that format so a later change to it
//! has to be deliberate.

use std::{collections::HashMap, time::Duration};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The number of random bytes behind a PKCE verifier and a state parameter.
///
/// RFC 7636 puts the verifier between 43 and 128 characters; 32 bytes encode
/// to 43, which is what the previous implementation used.
const RANDOM_BYTES: usize = 32;

/// A PKCE verifier and the challenge derived from it.
///
/// The verifier is held back until the code is exchanged, which is what makes
/// an intercepted authorization code useless on its own.
pub struct Pkce {
    verifier: String,
    challenge: String,
}

impl Pkce {
    /// Generates a fresh verifier and its S256 challenge.
    #[must_use]
    pub fn new_random() -> Self {
        Self::from_verifier(URL_SAFE_NO_PAD.encode(random_bytes()))
    }

    /// Derives the challenge for a given verifier.
    ///
    /// Split from [`Self::new_random`] so the derivation can be checked
    /// against RFC 7636's published verifier and challenge pair, which is
    /// only possible if the verifier can be supplied.
    fn from_verifier(verifier: String) -> Self {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Self {
            verifier,
            challenge,
        }
    }

    /// The verifier, to be sent when the code is redeemed.
    #[must_use]
    pub fn verifier(&self) -> &str {
        &self.verifier
    }
}

/// A random `state` parameter, base64url-encoded.
#[must_use]
pub fn random_state() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes())
}

/// Draws `RANDOM_BYTES` bytes from the OS.
///
/// # Panics
/// If the OS entropy source fails. There is no safe way to continue: the
/// alternative is a predictable PKCE verifier, which would leave the
/// authorization code exchangeable by anyone who observed it.
fn random_bytes() -> [u8; RANDOM_BYTES] {
    let mut bytes = [0u8; RANDOM_BYTES];
    getrandom::fill(&mut bytes).expect("OS entropy source unavailable");
    bytes
}

/// Builds the authorization request parameters.
///
/// Returned as a map rather than a query string because this flow does not
/// redirect a browser: the parameters are posted as JSON and the endpoint
/// replies with the redirect URL it would have sent.
#[must_use]
pub fn authorize_params(
    client_id: &str,
    redirect_uri: &str,
    scopes: &[&str],
    pkce: &Pkce,
    state: &str,
) -> HashMap<String, String> {
    [
        ("response_type", "code"),
        ("client_id", client_id),
        ("state", state),
        ("code_challenge", pkce.challenge.as_str()),
        // S256 is the only method offered. `plain` puts the verifier on the
        // wire, which defeats the point of sending a challenge at all.
        ("code_challenge_method", "S256"),
        ("redirect_uri", redirect_uri),
        ("scope", &scopes.join(" ")),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

/// The form body redeeming an authorization code.
#[must_use]
pub fn authorization_code_form(
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    pkce: &Pkce,
    state: Option<&str>,
) -> Vec<(String, String)> {
    let mut form = vec![
        ("grant_type".to_owned(), "authorization_code".to_owned()),
        ("code".to_owned(), code.to_owned()),
        ("code_verifier".to_owned(), pkce.verifier.clone()),
        // The client id travels in the body rather than a Basic auth header.
        // This client has no secret to authenticate with, and the endpoint
        // was configured to expect it here.
        ("client_id".to_owned(), client_id.to_owned()),
        ("redirect_uri".to_owned(), redirect_uri.to_owned()),
    ];
    if let Some(state) = state {
        form.push(("state".to_owned(), state.to_owned()));
    }
    form
}

/// The form body exchanging a refresh token for a new access token.
#[must_use]
pub fn refresh_token_form(client_id: &str, refresh_token: &str) -> Vec<(String, String)> {
    vec![
        ("grant_type".to_owned(), "refresh_token".to_owned()),
        ("refresh_token".to_owned(), refresh_token.to_owned()),
        ("client_id".to_owned(), client_id.to_owned()),
    ]
}

/// A successful token response.
///
/// `token_type` is accepted but unused: every token here is a bearer token,
/// and rejecting a response that omits the field would be a way to fail on a
/// token that works.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    /// Absent in principle; treated as immediately expiring, so the next call
    /// refreshes rather than sending a token of unknown lifetime.
    #[serde(default)]
    pub expires_in: Option<u64>,
    /// Absent when the server declines to rotate the refresh token.
    #[serde(default)]
    pub refresh_token: Option<String>,
}

impl TokenResponse {
    /// How long the access token remains valid, or zero if unstated.
    #[must_use]
    pub fn expires_in(&self) -> Duration {
        self.expires_in.map_or(Duration::ZERO, Duration::from_secs)
    }
}

/// An error response from the token endpoint, as defined by RFC 6749.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TokenErrorResponse {
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub error_description: Option<String>,
}

impl TokenErrorResponse {
    /// Whether the server is saying the refresh token is no longer usable.
    ///
    /// This decides between reporting a failure and starting a fresh
    /// authorization, so it errs towards recognising the condition: a
    /// refresh token that has been revoked or has expired is routine, and the
    /// cost of a wrong guess is one unnecessary re-authorization rather than
    /// a cookie stuck in a failing state.
    ///
    /// The description is consulted as well as the code because the endpoint
    /// has been seen returning the condition under a different code.
    #[must_use]
    pub fn is_invalid_grant(&self) -> bool {
        if self.error.to_lowercase().contains("invalid_grant") {
            return true;
        }
        self.error_description.as_ref().is_some_and(|desc| {
            let desc = desc.to_lowercase();
            desc.contains("refresh token not found")
                || desc.contains("refresh token") && desc.contains("invalid")
        })
    }
}

impl std::fmt::Display for TokenErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.error_description {
            Some(desc) => write!(f, "{}: {desc}", self.error),
            None => write!(f, "{}", self.error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact parameter set the previous implementation sent, recorded
    /// from it before the swap. The endpoint is a third-party server that
    /// cannot be exercised from a test, so this is the only thing standing
    /// between a rename here and a login failure in production.
    #[test]
    fn the_authorize_parameters_are_unchanged() {
        let pkce = Pkce::new_random();
        let params = authorize_params(
            "CLIENT_ID_X",
            "https://console.anthropic.com/oauth/code/callback",
            &["user:profile", "user:inference"],
            &pkce,
            "THESTATE",
        );

        let mut keys: Vec<_> = params.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "client_id",
                "code_challenge",
                "code_challenge_method",
                "redirect_uri",
                "response_type",
                "scope",
                "state",
            ]
        );
        assert_eq!(params["response_type"], "code");
        assert_eq!(params["client_id"], "CLIENT_ID_X");
        assert_eq!(params["code_challenge_method"], "S256");
        assert_eq!(params["state"], "THESTATE");
        assert_eq!(
            params["redirect_uri"],
            "https://console.anthropic.com/oauth/code/callback"
        );
        // Space-separated, as RFC 6749 requires -- not comma-separated, and
        // not repeated.
        assert_eq!(params["scope"], "user:profile user:inference");
    }

    /// Recorded from the previous implementation, including the client id in
    /// the body rather than an Authorization header.
    #[test]
    fn the_code_exchange_form_is_unchanged() {
        let pkce = Pkce::new_random();
        let form = authorization_code_form(
            "CLIENT_ID_X",
            "https://console.anthropic.com/oauth/code/callback",
            "THECODE",
            &pkce,
            Some("THESTATE"),
        );
        let got: HashMap<_, _> = form.iter().cloned().collect();

        assert_eq!(got["grant_type"], "authorization_code");
        assert_eq!(got["code"], "THECODE");
        assert_eq!(got["code_verifier"], pkce.verifier());
        assert_eq!(got["client_id"], "CLIENT_ID_X");
        assert_eq!(
            got["redirect_uri"],
            "https://console.anthropic.com/oauth/code/callback"
        );
        assert_eq!(got["state"], "THESTATE");
        assert_eq!(form.len(), 6);
    }

    /// The endpoint does not always return a state to echo, and the previous
    /// implementation omitted the parameter entirely in that case rather than
    /// sending it empty.
    #[test]
    fn the_state_parameter_is_omitted_when_absent() {
        let form = authorization_code_form("C", "R", "THECODE", &Pkce::new_random(), None);

        assert!(!form.iter().any(|(k, _)| k == "state"));
        assert_eq!(form.len(), 5);
    }

    #[test]
    fn the_refresh_form_is_unchanged() {
        let form = refresh_token_form("CLIENT_ID_X", "THEREFRESH");
        let got: HashMap<_, _> = form.iter().cloned().collect();

        assert_eq!(got["grant_type"], "refresh_token");
        assert_eq!(got["refresh_token"], "THEREFRESH");
        assert_eq!(got["client_id"], "CLIENT_ID_X");
        assert_eq!(form.len(), 3);
    }

    /// RFC 7636 appendix B's worked example, run through this code rather
    /// than recomputed alongside it. The challenge has to be the
    /// base64url-encoded SHA-256 of the verifier exactly -- unpadded, URL-safe
    /// alphabet -- or the server rejects every exchange.
    #[test]
    fn the_challenge_matches_the_rfc_test_vector() {
        let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".to_owned());

        assert_eq!(
            pkce.challenge,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    /// The challenge sent must correspond to the verifier kept back; if they
    /// ever came from different sources the exchange would fail.
    #[test]
    fn the_challenge_is_derived_from_the_verifier_it_keeps() {
        let pkce = Pkce::new_random();

        let rederived = Pkce::from_verifier(pkce.verifier().to_owned());
        assert_eq!(pkce.challenge, rederived.challenge);
        assert_ne!(pkce.challenge, pkce.verifier(), "S256, not plain");
    }

    /// The verifier must sit inside RFC 7636's length bounds, and must not
    /// carry base64 padding or non-URL-safe characters, both of which would
    /// have to be escaped in a form body.
    #[test]
    fn the_verifier_is_a_valid_pkce_verifier() {
        let pkce = Pkce::new_random();

        assert_eq!(pkce.verifier().len(), 43);
        assert!(
            pkce.verifier()
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "unreserved characters only: {}",
            pkce.verifier()
        );
    }

    #[test]
    fn each_verifier_and_state_is_distinct() {
        let verifiers: std::collections::HashSet<_> =
            (0..64).map(|_| Pkce::new_random().verifier).collect();
        let states: std::collections::HashSet<_> = (0..64).map(|_| random_state()).collect();

        assert_eq!(verifiers.len(), 64);
        assert_eq!(states.len(), 64);
    }

    #[test]
    fn a_token_response_is_parsed() {
        let json = r#"{"access_token":"AT","token_type":"bearer",
                       "expires_in":3600,"refresh_token":"RT","scope":"user:profile"}"#;

        let token: TokenResponse = serde_json::from_str(json).unwrap();

        assert_eq!(token.access_token, "AT");
        assert_eq!(token.expires_in(), Duration::from_hours(1));
        assert_eq!(token.refresh_token.as_deref(), Some("RT"));
    }

    /// Fields the endpoint adds later must not break parsing, and the two
    /// optional ones must not be required. `token_type` is deliberately
    /// tolerated when missing, which the previous implementation rejected --
    /// it is unused here, so failing on it would only reject a usable token.
    #[test]
    fn a_sparse_or_unfamiliar_token_response_is_accepted() {
        let json = r#"{"access_token":"AT","organization":{"uuid":"u"},"future_field":1}"#;

        let token: TokenResponse = serde_json::from_str(json).unwrap();

        assert_eq!(token.access_token, "AT");
        assert_eq!(token.expires_in(), Duration::ZERO);
        assert_eq!(token.refresh_token, None);
    }

    /// A token with no stated lifetime is treated as already expired, so the
    /// next request refreshes it instead of sending something the server may
    /// have already dropped.
    #[test]
    fn a_token_without_a_lifetime_expires_immediately() {
        let token = TokenResponse {
            access_token: "AT".into(),
            expires_in: None,
            refresh_token: None,
        };

        assert_eq!(token.expires_in(), Duration::ZERO);
    }

    /// Each of these was confirmed against the previous implementation's
    /// classifier; the two negative cases matter as much as the positive
    /// ones, since a wrong `true` throws away a working refresh token.
    #[test]
    fn invalid_grant_is_recognised_the_same_way() {
        let cases = [
            ("invalid_grant", None, true),
            ("invalid_grant", Some("Refresh token not found"), true),
            ("INVALID_GRANT", None, true),
            ("invalid_request", Some("refresh token is invalid"), true),
            ("invalid_request", Some("Refresh token not found"), true),
            ("invalid_client", None, false),
            ("invalid_request", None, false),
            ("server_error", Some("try again later"), false),
            ("invalid_request", Some("code is invalid"), false),
        ];

        for (error, description, expected) in cases {
            let response = TokenErrorResponse {
                error: error.to_owned(),
                error_description: description.map(str::to_owned),
            };

            assert_eq!(
                response.is_invalid_grant(),
                expected,
                "{error:?} / {description:?}"
            );
        }
    }

    #[test]
    fn an_error_response_is_parsed_and_displayed() {
        let json = r#"{"error":"invalid_grant","error_description":"Refresh token not found"}"#;

        let error: TokenErrorResponse = serde_json::from_str(json).unwrap();

        assert!(error.is_invalid_grant());
        assert_eq!(error.to_string(), "invalid_grant: Refresh token not found");
    }

    #[test]
    fn a_bare_error_response_is_parsed() {
        let error: TokenErrorResponse =
            serde_json::from_str(r#"{"error":"invalid_client"}"#).unwrap();

        assert!(!error.is_invalid_grant());
        assert_eq!(error.to_string(), "invalid_client");
    }
}
