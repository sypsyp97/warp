//! Sign-in-with-ChatGPT OAuth flow for the slim BYO LLM provider.
//!
//! Wire-compatible with the upstream Codex CLI flow
//! (<https://github.com/openai/codex>): Authorization Code + PKCE
//! against `auth.openai.com`, callback on `http://localhost:1455`,
//! resulting access token used as a `Bearer` against
//! `https://chatgpt.com/backend-api`.
//!
//! # TOS note
//!
//! Codex's hardcoded client_id is shipped verbatim. This is a TOS
//! gray area — OpenAI registered that client to Codex CLI, not to a
//! third-party fork — but the user has explicitly accepted that risk
//! for this slim build. Replace [`CODEX_CLIENT_ID`] with your own
//! `app_…` value if you'd rather register your own OAuth app.
//!
//! # Flow shape
//!
//! ```text
//!   start_authorization()
//!     -> (AuthFlow, auth_url)
//!     caller opens auth_url in the browser
//!   AuthFlow::wait_for_callback().await
//!     -> binds 127.0.0.1:1455
//!     -> serves a single GET /auth/callback
//!     -> validates state + extracts code
//!     -> POSTs token exchange against auth.openai.com
//!     -> returns CodexTokens { access_token, refresh_token, id_token, … }
//! ```
//!
//! Token refresh is exposed separately as [`refresh_codex_tokens`].

use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context as _, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_AUTH_BASE: &str = "https://auth.openai.com";
const CODEX_CALLBACK_PORT: u16 = 1455;
const CODEX_CALLBACK_PATH: &str = "/auth/callback";
const CODEX_SCOPES: &str = "openid profile email offline_access";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

/// The persisted result of a successful OAuth login. Stored in secure
/// storage; the access token is also mirrored into the BYO snapshot so
/// `byo_adapter` can pick it up without an `AppContext`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CodexTokens {
    pub access_token: String,
    /// `offline_access` scope is requested, so this is normally present
    /// — but per RFC 6749 §6 the server may omit it on refresh
    /// responses, in which case callers should carry forward the
    /// previous refresh token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Identity token that carries the `chatgpt_account_id` claim used
    /// by the backend to scope subscription quota. Optional because we
    /// don't strictly need it for plain chat — Bearer alone works.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    /// Unix epoch seconds at which `access_token` stops being valid.
    /// Zero means "unknown / treat as expired and refresh on first
    /// 401".
    #[serde(default)]
    pub expires_at: u64,
}

impl CodexTokens {
    pub fn is_access_token_likely_valid(&self) -> bool {
        if self.access_token.is_empty() {
            return false;
        }
        if self.expires_at == 0 {
            return true; // unknown lifetime — try it
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // 60-second cushion so we don't race the server.
        self.expires_at > now + 60
    }
}

/// In-flight OAuth state — created by [`start_authorization`], consumed
/// by [`AuthFlow::wait_for_callback`].
pub struct AuthFlow {
    code_verifier: String,
    state: String,
    redirect_uri: String,
}

/// Build the authorization URL the caller must open in the user's
/// browser. The returned [`AuthFlow`] carries the PKCE verifier and
/// CSRF state — keep it alive until the callback fires.
pub fn start_authorization() -> Result<(AuthFlow, String)> {
    let code_verifier = random_url_safe(64);
    let code_challenge = pkce_s256(&code_verifier);
    let state = random_url_safe(32);
    let redirect_uri = format!("http://localhost:{CODEX_CALLBACK_PORT}{CODEX_CALLBACK_PATH}");

    let auth_url = format!(
        "{CODEX_AUTH_BASE}/oauth/authorize\
         ?response_type=code\
         &client_id={client_id}\
         &redirect_uri={redirect}\
         &scope={scope}\
         &state={state}\
         &code_challenge={challenge}\
         &code_challenge_method=S256",
        client_id = urlencoding::encode(CODEX_CLIENT_ID),
        redirect = urlencoding::encode(&redirect_uri),
        scope = urlencoding::encode(CODEX_SCOPES),
        state = urlencoding::encode(&state),
        challenge = urlencoding::encode(&code_challenge),
    );

    Ok((
        AuthFlow {
            code_verifier,
            state,
            redirect_uri,
        },
        auth_url,
    ))
}

impl AuthFlow {
    /// Bind a one-shot listener on `127.0.0.1:1455`, accept exactly one
    /// `GET /auth/callback`, validate state + extract code, and
    /// exchange the code for tokens. Times out after 5 minutes.
    pub async fn wait_for_callback(self) -> Result<CodexTokens> {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), CODEX_CALLBACK_PORT);
        let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| {
            format!(
                "Could not bind {addr} for the OAuth callback. \
                 Make sure no other Codex / Warp / ChatGPT login flow \
                 is in progress."
            )
        })?;

        let callback = tokio::time::timeout(CALLBACK_TIMEOUT, accept_one_callback(&listener))
            .await
            .map_err(|_| anyhow!("OAuth flow timed out after 5 minutes"))??;

        if callback.state != self.state {
            bail!("OAuth state mismatch — refusing to proceed");
        }

        let code = callback
            .code
            .ok_or_else(|| anyhow!("OAuth callback missing 'code' parameter"))?;

        exchange_code_for_tokens(&code, &self.code_verifier, &self.redirect_uri).await
    }
}

/// Refresh an access token using a stored refresh token. RFC 6749 §6
/// allows the server to omit a new refresh_token from the response —
/// when that happens we carry the old one forward in the returned
/// `CodexTokens` so subsequent refreshes still work.
pub async fn refresh_codex_tokens(prev: &CodexTokens) -> Result<CodexTokens> {
    let refresh_token = prev
        .refresh_token
        .as_deref()
        .ok_or_else(|| anyhow!("No refresh_token stored — re-authenticate"))?;

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CODEX_CLIENT_ID),
    ];

    let response = reqwest::Client::new()
        .post(format!("{CODEX_AUTH_BASE}/oauth/token"))
        .form(&params)
        .send()
        .await
        .context("token refresh transport failure")?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("token refresh failed ({status}): {text}");
    }
    let parsed: TokenResponse = serde_json::from_str(&text)
        .with_context(|| format!("malformed token refresh response: {text}"))?;

    let mut tokens = CodexTokens::from(parsed);
    if tokens.refresh_token.is_none() {
        // Carry the previous refresh token forward — the server omitted
        // it, but the previous one is still valid.
        tokens.refresh_token = prev.refresh_token.clone();
    }
    Ok(tokens)
}

// ---------------------------------------------------------------------------
// internals
// ---------------------------------------------------------------------------

fn random_url_safe(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(&bytes)
}

fn pkce_s256(verifier: &str) -> String {
    let mut h = Sha256::new();
    h.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(h.finalize())
}

struct CallbackParams {
    code: Option<String>,
    state: String,
    error: Option<String>,
}

async fn accept_one_callback(listener: &tokio::net::TcpListener) -> Result<CallbackParams> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    loop {
        let (mut socket, _peer) = listener
            .accept()
            .await
            .context("accept on OAuth callback listener")?;

        let mut buf = [0u8; 8192];
        let n = socket
            .read(&mut buf)
            .await
            .context("read OAuth callback request")?;
        if n == 0 {
            continue;
        }
        let request = String::from_utf8_lossy(&buf[..n]);
        // First line: `GET /auth/callback?code=...&state=... HTTP/1.1`
        let request_line = request.lines().next().unwrap_or("");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("");
        let target = parts.next().unwrap_or("");

        // Anything that isn't the callback (favicon probes etc) gets
        // a 404 and the loop continues — we stay alive until the real
        // callback arrives or the timeout fires.
        if method != "GET" || !target.starts_with(CODEX_CALLBACK_PATH) {
            let _ = socket
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
            let _ = socket.shutdown().await;
            continue;
        }

        let params = parse_callback_query(target);
        let body = if let Some(err) = params.error.as_deref() {
            format!(
                "<!doctype html><meta charset=utf-8><title>Sign-in failed</title>\
                 <body style=\"font: 14px system-ui;padding:2em\">\
                 <h2>Sign-in failed</h2>\
                 <p>The OpenAI authorization server returned an error: \
                 <code>{}</code></p>\
                 <p>You can close this tab and try again from Warp.</p>",
                html_escape(err)
            )
        } else {
            "<!doctype html><meta charset=utf-8><title>Signed in</title>\
             <body style=\"font: 14px system-ui;padding:2em\">\
             <h2>Sign-in complete</h2>\
             <p>You can close this tab and return to Warp.</p>"
                .to_string()
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.shutdown().await;
        return Ok(params);
    }
}

fn parse_callback_query(target: &str) -> CallbackParams {
    // target = "/auth/callback?code=…&state=…&error=…" (any subset)
    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut code = None;
    let mut state = String::new();
    let mut error = None;
    for pair in query.split('&') {
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or("");
        let v = it.next().unwrap_or("");
        let v_decoded = urlencoding::decode(v).map(|c| c.into_owned()).ok();
        match k {
            "code" => code = v_decoded,
            "state" => state = v_decoded.unwrap_or_default(),
            "error" => error = v_decoded,
            _ => {}
        }
    }
    CallbackParams { code, state, error }
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

impl From<TokenResponse> for CodexTokens {
    fn from(r: TokenResponse) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let expires_at = r.expires_in.map(|secs| now + secs).unwrap_or(0);
        CodexTokens {
            access_token: r.access_token,
            refresh_token: r.refresh_token,
            id_token: r.id_token,
            expires_at,
        }
    }
}

async fn exchange_code_for_tokens(
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<CodexTokens> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", CODEX_CLIENT_ID),
        ("code_verifier", code_verifier),
    ];

    let response = reqwest::Client::new()
        .post(format!("{CODEX_AUTH_BASE}/oauth/token"))
        .form(&params)
        .send()
        .await
        .context("token exchange transport failure")?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("token exchange failed ({status}): {text}");
    }
    let parsed: TokenResponse = serde_json::from_str(&text)
        .with_context(|| format!("malformed token response: {text}"))?;
    Ok(CodexTokens::from(parsed))
}

// `Infallible` is used to mark the spawn-task error type when we don't
// expect it to fail; keeping the import here so a future async-task
// rework doesn't have to re-add it.
#[allow(dead_code)]
type _Phantom = Infallible;

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_spec_vector() {
        // RFC 7636 Appendix B test vector: verifier
        // "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk" → S256 challenge
        // "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = pkce_s256(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn parse_callback_query_extracts_code_and_state() {
        let p = parse_callback_query("/auth/callback?code=abc&state=xyz");
        assert_eq!(p.code.as_deref(), Some("abc"));
        assert_eq!(p.state, "xyz");
        assert!(p.error.is_none());
    }

    #[test]
    fn parse_callback_query_extracts_error_with_url_encoding() {
        let p = parse_callback_query("/auth/callback?error=access%20denied&state=s");
        assert_eq!(p.error.as_deref(), Some("access denied"));
        assert_eq!(p.state, "s");
        assert!(p.code.is_none());
    }

    #[test]
    fn random_url_safe_is_url_safe_and_correct_length() {
        let s = random_url_safe(32);
        // Base64 url-safe no-pad: 32 bytes -> ceil(32 * 4 / 3) = 43 chars
        assert_eq!(s.len(), 43);
        for c in s.chars() {
            assert!(
                c.is_ascii_alphanumeric() || c == '-' || c == '_',
                "non-url-safe char: {c}"
            );
        }
    }

    #[test]
    fn auth_url_carries_required_params() {
        let (flow, url) = start_authorization().unwrap();
        assert!(url.starts_with("https://auth.openai.com/oauth/authorize?"));
        assert!(url.contains(&format!("client_id={CODEX_CLIENT_ID}")));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains(&format!("state={}", urlencoding::encode(&flow.state))));
    }

    #[test]
    fn tokens_validity_horizon() {
        let mut t = CodexTokens::default();
        t.access_token = "x".into();
        // expires_at == 0 means "unknown" — treat as valid
        assert!(t.is_access_token_likely_valid());

        t.expires_at = 1; // long in the past
        assert!(!t.is_access_token_likely_valid());

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        t.expires_at = now + 3600;
        assert!(t.is_access_token_likely_valid());
    }
}
