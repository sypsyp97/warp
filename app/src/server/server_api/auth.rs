use std::{result::Result as StdResult, sync::Arc};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use cynic::{MutationBuilder, QueryBuilder};
use firebase::{FetchAccessTokenResponse, FirebaseError};
use instant::Duration;
#[cfg(test)]
use mockall::{automock, predicate::*};
use thiserror::Error;
use warp_core::errors::{AnyhowErrorExt, ErrorExt};
use warp_graphql::mutations::update_user_settings::{
    UpdateUserSettings, UpdateUserSettingsInput, UpdateUserSettingsResult,
    UpdateUserSettingsVariables,
};
use warp_graphql::queries::get_user_settings::{GetUserSettings, GetUserSettingsVariables};
use warpui::r#async::BoxFuture;

use crate::server::graphql::get_user_facing_error_message;
use crate::server::server_api::register_error;
use crate::settings::PrivacySettingsSnapshot;
use crate::{
    auth::credentials::{AuthToken, Credentials, FirebaseToken, LoginToken, RefreshToken},
    auth::user::FirebaseAuthTokens,
    channel::ChannelState,
    server::{
        datetime_ext::DateTimeExt as _, graphql::get_request_context,
        server_api::ServerApiEvent,
    },
};

use super::ServerApi;

/// Error messages returned from the Firebase REST API when attempting to convert a refresh token
/// into an access token that indicate the user's token is in an errored state.
/// These are "soft" errors because the user likely just needs to log in again.
/// See https://firebase.google.com/docs/reference/rest/auth#section-refresh-token.
static FETCH_ACCESS_TOKEN_SOFT_ERROR_MESSAGES: &[&str] = &[
    "TOKEN_EXPIRED",
    "INVALID_REFRESH_TOKEN",
    "MISSING_REFRESH_TOKEN",
];

/// Error messages returned from the Firebase REST API when attempting to convert a refresh token
/// into an access token that indicate the user's account is in an errored state.
/// These are "hard" errors because the user likely can no longer sign in with their account,
/// for example if it were disabled or deleted.
/// See https://firebase.google.com/docs/reference/rest/auth#section-refresh-token.
static FETCH_ACCESS_TOKEN_HARD_ERROR_MESSAGES: &[&str] = &["USER_DISABLED", "USER_NOT_FOUND"];

const FETCH_ACCESS_TOKEN_TIMEOUT: Duration = Duration::from_secs(5);

/// Header key for the ambient workload token attached to multi-agent requests.
pub const AMBIENT_WORKLOAD_TOKEN_HEADER: &str = "X-Warp-Ambient-Workload-Token";

/// Header key for the cloud agent task ID attached to requests from ambient agents.
pub const CLOUD_AGENT_ID_HEADER: &str = "X-Warp-Cloud-Agent-ID";

/// Duration for which the ambient workload token is valid (3 hours).
const AMBIENT_WORKLOAD_TOKEN_DURATION: Duration = Duration::from_secs(3 * 60 * 60);

/// User settings that are currently 'synced' (e.g. stored server-side) on a per-user basis.
#[derive(Copy, Clone, Debug, Default)]
pub struct SyncedUserSettings {
    pub is_cloud_conversation_storage_enabled: bool,
    pub is_crash_reporting_enabled: bool,
    pub is_telemetry_enabled: bool,
}

#[cfg_attr(test, automock)]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
pub trait AuthClient: 'static + Send + Sync {
    /// Returns the cached access token, if it is still valid. If it has expired, fetches a new
    /// access token using the user's refresh token, caches it, and the returns it.
    /// Returns an auth mode that may not require an Authorization header (e.g. session cookies or
    /// test credentials).
    async fn get_or_refresh_access_token(&self) -> Result<AuthToken>;

    /// Upon success, returns an `Option` containing the user's settings retrieved from the server,
    /// if any. The user may not have server-side settings if they onboarded prior to the launch
    /// of telemetry opt-out, have not logged in since the launch, and have never changed defaults
    /// for any of the settings in [`SyncedUserSettings`]. If the fetched settings object exists
    /// but is missing required fields, or if the request itself failed, returns an error.
    async fn get_user_settings(&self) -> Result<Option<SyncedUserSettings>>;

    async fn set_is_telemetry_enabled(&self, value: bool) -> Result<()>;

    async fn set_is_crash_reporting_enabled(&self, value: bool) -> Result<()>;

    async fn set_is_cloud_conversation_storage_enabled(&self, value: bool) -> Result<()>;

    /// Sends a request to update the user's settings on the server with values contained in the
    /// given `settings_snapshot`.
    async fn update_user_settings(&self, settings_snapshot: PrivacySettingsSnapshot) -> Result<()>;

    /// Returns a cached ambient workload token, or issues a new one if not present or expired.
    ///
    /// Returns `Ok(None)` if not running in an isolation platform (e.g., Namespace) or on WASM.
    async fn get_or_create_ambient_workload_token(&self) -> Result<Option<String>>;
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl AuthClient for ServerApi {
    async fn get_or_refresh_access_token(&self) -> Result<AuthToken> {
        // Slim fork: there is no Warp account. We never expect this to
        // be hit on the hot path because every call site is gated on
        // `is_logged_in()`, but if it does fire we fail quietly with a
        // distinct message that consumers can match on if they want to
        // demote the log level.
        let Some(credentials) = self.auth_state.credentials() else {
            bail!("slim fork has no Warp account; authenticated request skipped");
        };

        match credentials {
            Credentials::ApiKey { key, .. } => Ok(AuthToken::ApiKey(key)),
            Credentials::Firebase(auth_tokens) => {
                let expiration_time = auth_tokens.expiration_time;

                // Generate a new ID token if the token has expired or will expire in the
                // next five minutes. This matches the behavior of the Firebase Auth SDK.
                if chrono::DateTime::now() + chrono::Duration::minutes(5) >= expiration_time {
                    let refresh_token = auth_tokens.refresh_token.clone();
                    let firebase_token = FirebaseToken::Refresh(RefreshToken::new(refresh_token));

                    let result = fetch_auth_tokens(self.client.clone(), firebase_token).await;

                    if let Err(UserAuthenticationError::DeniedAccessToken(_)) = result {
                        let _ = self.event_sender.send(ServerApiEvent::NeedsReauth).await;
                    }
                    let new_firebase_token_info = result?;
                    self.auth_state
                        .update_firebase_tokens(new_firebase_token_info.clone());
                    return Ok(AuthToken::Firebase(new_firebase_token_info.id_token));
                }

                Ok(AuthToken::Firebase(auth_tokens.id_token))
            }
            Credentials::SessionCookie => Ok(AuthToken::NoAuth),
            #[cfg(any(test, feature = "integration_tests", feature = "skip_login"))]
            Credentials::Test => Ok(AuthToken::NoAuth),
        }
    }

    async fn get_user_settings(&self) -> Result<Option<SyncedUserSettings>> {
        let variables = GetUserSettingsVariables {
            request_context: get_request_context(),
        };
        let operation = GetUserSettings::build(variables);
        let response = self.send_graphql_request(operation, None).await?;

        match response.user {
            warp_graphql::queries::get_user_settings::UserResult::UserOutput(user_output) => {
                match user_output.user.settings {
                    Some(user_settings) => Ok(Some(SyncedUserSettings {
                        is_cloud_conversation_storage_enabled: user_settings
                            .is_cloud_conversation_storage_enabled,
                        is_crash_reporting_enabled: user_settings.is_crash_reporting_enabled,
                        is_telemetry_enabled: user_settings.is_telemetry_enabled,
                    })),
                    None => Ok(None),
                }
            }
            warp_graphql::queries::get_user_settings::UserResult::Unknown => {
                Err(anyhow!("Unable to fetch user settings"))
            }
        }
    }

    async fn set_is_telemetry_enabled(&self, value: bool) -> Result<()> {
        let variables = UpdateUserSettingsVariables {
            input: UpdateUserSettingsInput {
                telemetry_enabled: Some(value),
                ..Default::default()
            },
            request_context: get_request_context(),
        };

        let operation = UpdateUserSettings::build(variables);
        let result = self
            .send_graphql_request(operation, None)
            .await?
            .update_user_settings;

        match result {
            UpdateUserSettingsResult::UpdateUserSettingsOutput(_) => Ok(()),
            UpdateUserSettingsResult::UserFacingError(user_facing_error) => {
                Err(anyhow!(get_user_facing_error_message(user_facing_error)))
            }
            UpdateUserSettingsResult::Unknown => Err(anyhow!("failed to set telemetry enabled")),
        }
    }

    async fn set_is_crash_reporting_enabled(&self, value: bool) -> Result<()> {
        let variables = UpdateUserSettingsVariables {
            input: UpdateUserSettingsInput {
                crash_reporting_enabled: Some(value),
                ..Default::default()
            },
            request_context: get_request_context(),
        };

        let operation = UpdateUserSettings::build(variables);
        let result = self
            .send_graphql_request(operation, None)
            .await?
            .update_user_settings;

        match result {
            UpdateUserSettingsResult::UpdateUserSettingsOutput(_) => Ok(()),
            UpdateUserSettingsResult::UserFacingError(user_facing_error) => {
                Err(anyhow!(get_user_facing_error_message(user_facing_error)))
            }
            UpdateUserSettingsResult::Unknown => {
                Err(anyhow!("failed to set crash reporting enabled"))
            }
        }
    }

    async fn set_is_cloud_conversation_storage_enabled(&self, value: bool) -> Result<()> {
        let variables = UpdateUserSettingsVariables {
            input: UpdateUserSettingsInput {
                cloud_conversation_storage_enabled: Some(value),
                ..Default::default()
            },
            request_context: get_request_context(),
        };

        let operation = UpdateUserSettings::build(variables);
        let result = self
            .send_graphql_request(operation, None)
            .await?
            .update_user_settings;

        match result {
            UpdateUserSettingsResult::UpdateUserSettingsOutput(_) => Ok(()),
            UpdateUserSettingsResult::UserFacingError(user_facing_error) => {
                Err(anyhow!(get_user_facing_error_message(user_facing_error)))
            }
            UpdateUserSettingsResult::Unknown => {
                Err(anyhow!("failed to set cloud conversation storage enabled"))
            }
        }
    }

    async fn update_user_settings(&self, settings_snapshot: PrivacySettingsSnapshot) -> Result<()> {
        let variables = UpdateUserSettingsVariables {
            input: UpdateUserSettingsInput {
                telemetry_enabled: Some(settings_snapshot.is_telemetry_enabled()),
                crash_reporting_enabled: Some(settings_snapshot.is_crash_reporting_enabled()),
                cloud_conversation_storage_enabled: settings_snapshot
                    .cloud_conversation_storage_enabled(),
            },
            request_context: get_request_context(),
        };

        let operation = UpdateUserSettings::build(variables);
        let result = self
            .send_graphql_request(operation, None)
            .await?
            .update_user_settings;

        match result {
            UpdateUserSettingsResult::UpdateUserSettingsOutput(_) => Ok(()),
            UpdateUserSettingsResult::UserFacingError(user_facing_error) => {
                Err(anyhow!(get_user_facing_error_message(user_facing_error)))
            }
            UpdateUserSettingsResult::Unknown => Err(anyhow!("failed to update user settings")),
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

/// Exchange a long-lived token for fresh [`Credentials`].
#[allow(dead_code)]
async fn exchange_credentials(
    client: Arc<http_client::Client>,
    token: LoginToken,
) -> StdResult<Credentials, UserAuthenticationError> {
    match token {
        LoginToken::Firebase(firebase_token) => {
            let tokens = fetch_auth_tokens(client, firebase_token).await?;
            Ok(Credentials::Firebase(tokens))
        }
        LoginToken::ApiKey(key) => Ok(Credentials::ApiKey {
            key,
            owner_type: None,
        }),
        LoginToken::SessionCookie => Ok(Credentials::SessionCookie),
    }
}

fn fetch_auth_tokens(
    client: Arc<http_client::Client>,
    token: FirebaseToken,
) -> BoxFuture<'static, StdResult<FirebaseAuthTokens, UserAuthenticationError>> {
    Box::pin(async move {
        let firebase_api_key = ChannelState::firebase_api_key();
        let url = token.access_token_url(&firebase_api_key);
        let request_body = token.access_token_request_body();
        let proxy_url = token.proxy_url(&ChannelState::server_root_url(), &firebase_api_key);
        let response = match client
            .post(&url)
            .form(&request_body)
            .timeout(FETCH_ACCESS_TOKEN_TIMEOUT)
            .send()
            .await
        {
            Ok(response) => match response.error_for_status_ref() {
                Ok(_) => Ok(response),
                Err(error) => {
                    log::warn!(
                        "Request to firebase to fetch access token completed, but was unsuccessful: {error:?}"
                    );

                    fetch_access_token_via_proxy(client, &request_body, proxy_url).await
                }
            },
            Err(error) => {
                log::warn!("Failed to make response to firebase to fetch access token: {error:?}");

                fetch_access_token_via_proxy(client, &request_body, proxy_url).await
            }
        }?;

        let response = response
            .json::<FetchAccessTokenResponse>()
            .await
            .map_err(anyhow::Error::from)?;
        match response {
            FetchAccessTokenResponse::Success {
                id_token,
                expires_in,
                refresh_token,
            } => Ok(FirebaseAuthTokens::from_response(
                id_token,
                refresh_token,
                expires_in,
            )?),
            FetchAccessTokenResponse::Error { error } => Err(error.into()),
        }
    })
}

fn fetch_access_token_via_proxy<'a>(
    client: Arc<http_client::Client>,
    request_body: &'a [(&'a str, &'a str)],
    proxy_url: String,
) -> BoxFuture<'a, Result<http_client::Response>> {
    Box::pin(async move {
        client
            .post(&proxy_url)
            .form(request_body)
            .send()
            .await
            .map_err(anyhow::Error::from)
    })
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

impl From<FirebaseError> for UserAuthenticationError {
    fn from(error: FirebaseError) -> Self {
        if FETCH_ACCESS_TOKEN_SOFT_ERROR_MESSAGES.contains(&error.message.as_str()) {
            UserAuthenticationError::DeniedAccessToken(error)
        } else if FETCH_ACCESS_TOKEN_HARD_ERROR_MESSAGES.contains(&error.message.as_str()) {
            UserAuthenticationError::UserAccountDisabled(error)
        } else {
            UserAuthenticationError::Unexpected(
                anyhow::Error::from(error)
                    .context("Failed to exchange refresh token with access token."),
            )
        }
    }
}

#[derive(Error, Debug)]
/// Error type when minting a new custom token for an anonymous user
#[allow(dead_code)]
pub enum MintCustomTokenError {
    #[error("Received a user facing error: {0}")]
    UserFacingError(String),
    #[error("Failed to create new custom token with unknown error")]
    Unknown,
}

#[cfg(test)]
#[path = "auth_test.rs"]
mod tests;
