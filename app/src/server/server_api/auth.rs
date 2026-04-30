use anyhow::{bail, Result};
use async_trait::async_trait;
use firebase::FirebaseError;
use instant::Duration;
#[cfg(test)]
use mockall::{automock, predicate::*};
use thiserror::Error;
use warp_core::errors::{AnyhowErrorExt, ErrorExt};

use crate::auth::credentials::{AuthToken, Credentials};
use crate::server::server_api::register_error;

use super::ServerApi;

/// Header key for the ambient workload token attached to multi-agent requests.
pub const AMBIENT_WORKLOAD_TOKEN_HEADER: &str = "X-Warp-Ambient-Workload-Token";

/// Header key for the cloud agent task ID attached to requests from ambient agents.
pub const CLOUD_AGENT_ID_HEADER: &str = "X-Warp-Cloud-Agent-ID";

/// Duration for which the ambient workload token is valid (3 hours).
const AMBIENT_WORKLOAD_TOKEN_DURATION: Duration = Duration::from_secs(3 * 60 * 60);

#[cfg_attr(test, automock)]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
pub trait AuthClient: 'static + Send + Sync {
    /// Returns the cached access token, if it is still valid. If it has expired, fetches a new
    /// access token using the user's refresh token, caches it, and the returns it.
    /// Returns an auth mode that may not require an Authorization header (e.g. session cookies or
    /// test credentials).
    async fn get_or_refresh_access_token(&self) -> Result<AuthToken>;

    /// Returns a cached ambient workload token, or issues a new one if not present or expired.
    ///
    /// Returns `Ok(None)` if not running in an isolation platform (e.g., Namespace) or on WASM.
    async fn get_or_create_ambient_workload_token(&self) -> Result<Option<String>>;
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl AuthClient for ServerApi {
    async fn get_or_refresh_access_token(&self) -> Result<AuthToken> {
        // Slim fork: every authenticated request is gated on `is_logged_in()`,
        // and the only credentials we ever construct in production are
        // `Credentials::ApiKey`. The Firebase / SessionCookie variants still
        // exist on the enum so tests and persistence types compile, but
        // hitting them here means something tried to make a Warp cloud call
        // we can't service.
        let Some(credentials) = self.auth_state.credentials() else {
            bail!("slim fork has no Warp account; authenticated request skipped");
        };

        match credentials {
            Credentials::ApiKey { key, .. } => Ok(AuthToken::ApiKey(key)),
            Credentials::Firebase(_) | Credentials::SessionCookie => {
                bail!("slim fork has no Firebase/SessionCookie auth path")
            }
            #[cfg(any(test, feature = "integration_tests", feature = "skip_login"))]
            Credentials::Test => Ok(AuthToken::NoAuth),
        }
    }

    async fn get_or_create_ambient_workload_token(&self) -> Result<Option<String>> {
        if cfg!(target_family = "wasm") {
            return Ok(None);
        }

        // Check if we have a cached token that's still valid (with 5 minute buffer).
        // Tokens without an expiration time are always considered valid.
        {
            let cached = self.ambient_workload_token.lock();
            if let Some(ref token) = *cached {
                let is_valid = token.expires_at.is_none_or(|expires_at| {
                    chrono::Utc::now() + chrono::Duration::minutes(5) < expires_at
                });
                if is_valid {
                    return Ok(Some(token.token.clone()));
                }
            }
        }

        // Issue a new token.
        let workload_token = match warp_isolation_platform::issue_workload_token(Some(
            AMBIENT_WORKLOAD_TOKEN_DURATION,
        ))
        .await
        {
            Ok(token) => token,
            Err(warp_isolation_platform::IsolationPlatformError::NoIsolationPlatformDetected) => {
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        };

        let token_str = workload_token.token.clone();

        {
            let mut cached = self.ambient_workload_token.lock();
            *cached = Some(workload_token);
        }

        Ok(Some(token_str))
    }
}

/// The [`oauth2::Client`] type, specialized to the endpoints that we require.
pub type OAuth2Client = oauth2::basic::BasicClient<
    oauth2::EndpointNotSet, // HasAuthUrl
    oauth2::EndpointSet,    // HasDeviceAuthUrl
    oauth2::EndpointNotSet, // HasIntrospectionUrl
    oauth2::EndpointNotSet, // HasRevocationUrl
    oauth2::EndpointSet,    // HasTokenUrl
>;


#[derive(Error, Debug)]
/// Error type when retrieving a user and validating it against Firebase.
pub enum UserAuthenticationError {
    /// The user's refresh token is invalid. This could occur if the user authed through
    /// e.g. Google/GitHub and changed their password.
    #[error("Firebase returned a token error when fetching an ID token")]
    DeniedAccessToken(FirebaseError),
    /// The user's account is invalid. This could occur if the user requested their account
    /// be deleted per their GDPR/CCPA rights.
    #[allow(dead_code)]
    #[error("Firebase returned a user error when fetching an ID token")]
    UserAccountDisabled(FirebaseError),
    #[allow(dead_code)]
    #[error("Invalid state parameter in auth redirect")]
    InvalidStateParameter,
    #[allow(dead_code)]
    #[error("Missing state parameter in auth redirect")]
    MissingStateParameter,
    #[error("unexpected error occurred when fetching an ID token: {0:#}")]
    Unexpected(#[from] anyhow::Error),
}

impl ErrorExt for UserAuthenticationError {
    fn is_actionable(&self) -> bool {
        match self {
            UserAuthenticationError::DeniedAccessToken(err) => {
                // If a request to our server failed because the user's refresh token
                // has expired, they should re-auth, but there's no value in reporting
                // this back to us.
                log::info!("ignoring denied access token error: {err:#}");
                false
            }
            UserAuthenticationError::UserAccountDisabled(err) => {
                // Similarly, if their account is disabled, they can't make requests.
                log::info!("ignoring user account disabled error: {err:#}");
                false
            }
            UserAuthenticationError::Unexpected(err) => err.is_actionable(),
            UserAuthenticationError::InvalidStateParameter
            | UserAuthenticationError::MissingStateParameter => {
                // For now, we're marking these as actionable, since a surplus of these errors
                // could mean that something is wrong in our login flow (e.g. we're not properly
                // passing the `state` variable back to the desktop client).
                // But in general, someone attempting to trick another into logging into their
                // account with a spoofed `state` variable is not actionable.
                true
            }
        }
    }
}
register_error!(UserAuthenticationError);

