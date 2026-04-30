pub(super) mod user_persistence;

use std::result::Result as StdResult;
use std::sync::Arc;

use settings::Setting as _;
use uuid::Uuid;
use warp_core::features::FeatureFlag;
use warpui::{Entity, ModelContext, SingletonEntity, UpdateModel};

use super::auth_state::{AuthState, PersistAction};
use super::auth_view_modal::{AuthRedirectPayload, AuthViewVariant};
use super::credentials::Credentials;
use super::user::User;
use super::AuthStateProvider;
use super::UserUid;
use crate::ai::llms::LLMPreferences;
use crate::ai::persisted_workspace::PersistedWorkspace;
use crate::ai::AIRequestUsageModel;
use crate::autoupdate::AutoupdateState;
use crate::persistence::ModelEvent;
use crate::server::cloud_objects::update_manager::UpdateManager;
use crate::server::server_api::auth::FetchUserResult;
use crate::server::server_api::ServerApiProvider;
use crate::server::{
    server_api::{
        auth::{AuthClient, MintCustomTokenError, UserAuthenticationError},
        ServerApi,
    },
    telemetry::AnonymousUserSignupEntrypoint,
};
use crate::settings::cloud_preferences_syncer::CloudPreferencesSyncer;
use crate::settings::initializer::SettingsInitializer;
use crate::settings::PrivacySettings;
use crate::terminal::general_settings::GeneralSettings;
use crate::terminal::shared_session::manager::Manager as SharedSessionManager;
use crate::workspaces::team_tester::TeamTesterStatus;
use crate::{
    persistence, report_if_error, send_telemetry_from_ctx, GlobalResourceHandlesProvider,
    TelemetryEvent,
};
use user_persistence::PersistedUser;

#[derive(Debug)]
pub enum AuthManagerEvent {
    /// Successfully authenticated a user with no errors.
    AuthComplete,
    /// Failed to authenticate a user, due to a particular `UserAuthenticationError`.
    #[allow(dead_code)]
    AuthFailed(UserAuthenticationError),
    /// Failed to create an anonymous user.
    #[allow(dead_code)]
    CreateAnonymousUserFailed,
    /// The user chose to skip login entirely (no Firebase user created).
    SkippedLogin,
    /// The user now needs to reauthenticate. If the user needs to reauth, an `AuthFailed`
    /// event might be triggered instead, but there are some code paths where we don't
    /// refresh the entire user, only their token, which is when this event might be emitted.
    NeedsReauth,
    /// The user is anonymous and has attempted to access a login-gated feature or link.
    #[allow(dead_code)]
    AttemptedLoginGatedFeature {
        auth_view_variant: AuthViewVariant,
    },
    // The current user is anonymous and the client has received a browser intent to sign in with a different Warp account.
    // Holds an auth payload from the received browser intent.
    #[allow(dead_code)]
    LoginOverrideDetected(AuthRedirectPayload),
    /// Failed to mint a new custom token for an anonymous user.
    #[allow(dead_code)]
    MintCustomTokenFailed(MintCustomTokenError),
    /// Received a device authorization code as part of the device auth flow.
    #[allow(dead_code)]
    ReceivedDeviceAuthorizationCode {
        #[cfg_attr(target_family = "wasm", allow(unused))]
        verification_url: String,
        #[cfg_attr(target_family = "wasm", allow(unused))]
        verification_url_complete: Option<String>,
        #[cfg_attr(target_family = "wasm", allow(unused))]
        user_code: String,
    },
}

pub type LoginGatedFeature = &'static str;

type URLConstructorCallback = Box<dyn FnOnce(Option<&str>) -> String>;

/// AuthManager is a singleton model which manages the currently logged-in user's state.
/// If you need to access the state, use `AuthStateProvider`.
pub struct AuthManager {
    auth_state: Arc<AuthState>,
    server_api: Arc<ServerApi>,
    auth_client: Arc<dyn AuthClient>,
    /// A generated state token that the web app must provide back to the client.
    pending_auth_state: Option<String>,
}

impl AuthManager {
    /// Creates a new instance of the AuthManager. The auth state must already be initialized through
    /// [`AuthStateProvider`].
    pub fn new(
        server_api: Arc<ServerApi>,
        auth_client: Arc<dyn AuthClient>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let auth_state = AuthStateProvider::as_ref(ctx).get().clone();

        Self {
            auth_state,
            server_api,
            auth_client,
            pending_auth_state: None,
        }
    }

    #[cfg(test)]
    pub fn new_for_test(ctx: &mut ModelContext<Self>) -> Self {
        use crate::server::server_api::ServerApiProvider;

        let server_api = ServerApiProvider::as_ref(ctx).get();
        let auth_state = AuthStateProvider::as_ref(ctx).get().clone();

        Self {
            auth_state,
            server_api: server_api.clone(),
            auth_client: server_api,
            pending_auth_state: None,
        }
    }

    /// Slim fork: there is no `warp.dev` redirect to process, so the entire
    /// browser-intent → fetch-user pipeline is short-circuited.
    pub fn initialize_user_from_auth_payload(
        &mut self,
        _auth_payload: AuthRedirectPayload,
        _enforce_state_validation: bool,
        _ctx: &mut ModelContext<Self>,
    ) {
    }

    pub fn resume_interrupted_auth_payload(
        &mut self,
        _auth_payload: AuthRedirectPayload,
        _ctx: &mut ModelContext<Self>,
    ) {
    }

    #[cfg(target_family = "wasm")]
    pub fn initialize_user_from_session_cookie(&self, ctx: &mut ModelContext<Self>) {
        let auth_client = self.auth_client.clone();
        let _ = ctx.spawn(
            async move {
                auth_client
                    .fetch_user(LoginToken::SessionCookie, false)
                    .await
            },
            Self::on_user_fetched,
        );
    }

    /// Refreshes the user's auth state using their existing credentials.
    pub fn refresh_user(&self, ctx: &mut ModelContext<Self>) {
        let Some(credentials) = self.auth_state.credentials() else {
            log::warn!("Attempted to refresh user without credentials");
            return;
        };

        let Some(token) = credentials.login_token() else {
            // Slim fork: with no Warp account this is the normal state.
            log::debug!("No login token; user refresh skipped");
            return;
        };

        let auth_client = self.auth_client.clone();
        let _ = ctx.spawn(
            async move { auth_client.fetch_user(token, true).await },
            Self::on_user_fetched,
        );
    }

    /// Slim fork: device auth flow is dead. CLI `warp login` sub-commands
    /// remain wired so the binary still type-checks, but they never
    /// produce credentials.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub fn authorize_device(&self, _ctx: &mut ModelContext<Self>) {}

    /// Callback for handling a successful fetch of a user from warp-server and Firebase.
    /// This does the heavy-lifting of setting up all components of the application that depend
    /// on a user's authenticated state, and emits events to subscribers that let them know
    /// an auth event has occurred.
    fn on_user_fetched(
        &mut self,
        fetch_user_result: StdResult<FetchUserResult, UserAuthenticationError>,
        ctx: &mut ModelContext<Self>,
    ) {
        match fetch_user_result {
            Ok(fetch_user_result) => {
                let FetchUserResult {
                    user,
                    credentials,
                    server_experiments,
                    from_refresh,
                    llms,
                } = fetch_user_result;

                self.set_and_persist(Some(user.clone()), Some(credentials), ctx);

                self.set_needs_reauth(false, ctx);

                ServerApiProvider::handle(ctx).update(ctx, |provider, ctx| {
                    provider.handle_experiments_fetched(server_experiments, ctx);
                });

                SettingsInitializer::handle(ctx).update(ctx, |initializer, ctx| {
                    initializer.handle_user_fetched(self.auth_state.clone(), ctx);
                });

                // Reset the initial-load condition so that any cloud preference
                // sync waits for the *new* user's cloud objects rather than
                // resolving immediately against stale data from a prior session.
                // Only do this for non-refresh fetches (login/signup), not for
                // token refreshes where the user identity hasn't changed.
                if !from_refresh {
                    UpdateManager::handle(ctx).update(ctx, |manager, _| {
                        manager.reset_initial_load();
                    });
                }

                // Now that we have a user, start polling for team and cloud object information.
                // The polling loop's first tick fires immediately, so there is no need for a
                // separate out-of-band refresh here.
                TeamTesterStatus::handle(ctx).update(ctx, |model, ctx| {
                    model.initiate_data_pollers(false, ctx);
                });

                CloudPreferencesSyncer::handle(ctx).update(ctx, |model, ctx| {
                    model.handle_user_fetched(self.auth_state.clone(), ctx)
                });

                AIRequestUsageModel::handle(ctx).update(ctx, |usage_model, ctx| {
                    usage_model.refresh_request_usage_async(ctx);
                });

                LLMPreferences::handle(ctx).update(ctx, |prefs, ctx| {
                    prefs.update_feature_model_choices(Ok(llms), ctx);
                });

                PersistedWorkspace::handle(ctx).update(ctx, |index_manager_updater, ctx| {
                    index_manager_updater.on_user_changed(ctx);
                });

                if !user.is_user_anonymous() {
                    GeneralSettings::handle(ctx).update(ctx, |settings, ctx| {
                        report_if_error!(settings
                            .did_non_anonymous_user_log_in
                            .set_value(true, ctx));
                    });
                }

                // Force refresh for shared sessions if user may have changed.
                if !from_refresh {
                    SharedSessionManager::handle(ctx).update(ctx, |manager, ctx| {
                        manager.stop_all_shared_sessions(ctx);
                        manager.rejoin_all_shared_sessions(ctx);
                    });
                }

                let global_resource_handles =
                    GlobalResourceHandlesProvider::as_ref(ctx).get().clone();

                // As part of Logout v0:
                // Reconstruct the database if it was removed.
                // Do nothing if the database was not removed.
                persistence::reconstruct(&global_resource_handles.model_event_sender);
                if let Some(model_event_sender) = &global_resource_handles.model_event_sender {
                    if let Err(e) =
                        model_event_sender.send(ModelEvent::UpsertCurrentUserInformation {
                            user_information: PersistedCurrentUserInformation {
                                email: self.auth_state.user_email().unwrap_or_default(),
                            },
                        })
                    {
                        log::error!("Error persisting user information to database: {e:?}");
                    };
                }

                // Fetch the user's privacy settings from the server if any or update the server settings.
                let privacy_settings_handle = PrivacySettings::handle(ctx);
                let privacy_settings_snapshot =
                    privacy_settings_handle.as_ref(ctx).get_snapshot(ctx);
                ctx.update_model(&privacy_settings_handle, |privacy_settings, ctx| {
                    privacy_settings.fetch_or_update_settings(ctx);
                });

                // Now that the user is logged in, do the daily version check.
                if FeatureFlag::Autoupdate.is_enabled() {
                    AutoupdateState::handle(ctx).update(ctx, |autoupdate_state, ctx| {
                        autoupdate_state.maybe_daily_check_for_update(ctx);
                    });
                }

                let server_api = self.server_api.clone();
                let user_id = self.auth_state.user_id().unwrap_or_default();
                let anonymous_id = self.auth_state.anonymous_id();
                let _ = ctx.spawn(
                    // Synchronously add the identify and login event to the telemetry event queue and
                    // then flush the queue to ensure the events get to Rudderstack. We need to do this
                    // one-off because the login event happens only once for the user and we don't want
                    // to drop the event if the user quits the app before the next flush of the queue.
                    // TODO(alokedesai): Investigate a more robust way of handling events
                    // that don't get flushed to Rudderstack outside of this event specifically.
                    async move {
                        warpui::telemetry::record_identify_user_event(
                            user_id.as_string(),
                            anonymous_id.clone(),
                            warpui::time::get_current_time(),
                        );
                        warpui::telemetry::record_event(
                            Some(user_id.as_string()),
                            anonymous_id,
                            TelemetryEvent::Login.name().into(),
                            TelemetryEvent::Login.payload(),
                            TelemetryEvent::Login.contains_ugc(),
                            warpui::time::get_current_time(),
                        );

                        // Note that this snapshot might get overwritten to disabled after the server fetch.
                        // However, it is still fine to flush to Rudderstack here as the login event is low-risk
                        // and it is better to err on the side of over-reporting than under-reporting.
                        if let Err(e) = server_api
                            .flush_telemetry_events(privacy_settings_snapshot)
                            .await
                        {
                            log::info!("Failed to flush events from Telemetry queue: {e}");
                        }
                        server_api.notify_login().await;
                    },
                    |_, _, _| {},
                );

                // Once the user is authenticated, attempt to report the sandbox that Warp is running in, if any.
                ctx.spawn(
                    async { warp_isolation_platform::detect() },
                    |_, platform, ctx| {
                        if let Some(platform) = platform {
                            send_telemetry_from_ctx!(
                                TelemetryEvent::DetectedIsolationPlatform { platform },
                                ctx
                            );
                        }
                    },
                );

                ctx.emit(AuthManagerEvent::AuthComplete);
            }
            Err(error) => {
                match error {
                    UserAuthenticationError::DeniedAccessToken(_) => {
                        self.set_needs_reauth(true, ctx);
                    }
                    UserAuthenticationError::UserAccountDisabled(_) => {}
                    UserAuthenticationError::Unexpected(_) => {}
                    UserAuthenticationError::InvalidStateParameter => {}
                    UserAuthenticationError::MissingStateParameter => {}
                }

                ctx.emit(AuthManagerEvent::AuthFailed(error));
            }
        }
    }

    /// Sets the user and credentials in auth state and persists to secure storage.
    /// Persistence depends on the credential type - currently, we only persist
    /// state if authenticated via a Firebase token.
    fn set_and_persist(
        &self,
        user: Option<User>,
        credentials: Option<Credentials>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.auth_state.set_user(user);
        self.auth_state.set_credentials(credentials);
        self.persist(ctx);
    }

    /// Persists (or removes) the current user and credentials to/from secure storage,
    /// based on the current auth state.
    fn persist(&self, ctx: &mut ModelContext<Self>) {
        match self.auth_state.persist_action() {
            PersistAction::Persist(persisted_user) => {
                if persisted_user.auth_tokens.refresh_token.is_empty() {
                    log::warn!("Skipping user persistence due to empty refresh token");
                    return;
                }
                let _ = persisted_user.write_to_secure_storage(ctx).map_err(|err| {
                    log::warn!("Unable to persist user to secure storage: {err:?}");
                });
            }
            PersistAction::Remove => {
                let _ = PersistedUser::remove_from_secure_storage(ctx).map_err(|err| {
                    log::warn!("Unable to clear user from secure storage: {err:?}");
                });
            }
            PersistAction::DoNothing => {}
        }
    }

    /// Helper function for logging out the user.
    /// NOTE: You probably want to call auth::log_out instead; this only manages the auth state,
    /// it doesn't shut down any other user-dependent parts of the app.
    /// TODO(jeff): Can we move those pieces in here?
    pub(super) fn log_out(&mut self, ctx: &mut ModelContext<Self>) {
        // Clear any dangling CSRF token from an auth flow that was started but never
        // completed before this logout, so it can't be replayed against the next session
        // in the same process.
        self.pending_auth_state = None;
        self.set_and_persist(None, None, ctx);
    }

    /// Sets whether or not this user's Firebase credentials are invalid and thus needs to reauth.
    pub fn set_needs_reauth(&self, needs_reauth: bool, ctx: &mut ModelContext<Self>) {
        let became_true = self.auth_state.set_needs_reauth(needs_reauth);

        if became_true {
            send_telemetry_from_ctx!(TelemetryEvent::NeedsReauth, ctx);
            ctx.emit(AuthManagerEvent::NeedsReauth);
        }
    }

    /// Slim fork: anonymous user creation is dead — there is no Warp
    /// account to anonymously stand in for.
    pub fn create_anonymous_user(
        &self,
        _referral_code: Option<String>,
        _ctx: &mut ModelContext<Self>,
    ) {
    }

    /// Slim fork: every "login-gated" feature is permanently denied without
    /// any modal popup, since there is no login flow. Callers continue to
    /// invoke this; nothing happens.
    pub fn attempt_login_gated_feature(
        &self,
        _feature: LoginGatedFeature,
        _auth_view_variant: AuthViewVariant,
        _ctx: &mut ModelContext<Self>,
    ) {
    }

    pub fn anonymous_user_hit_drive_object_limit(&self, _ctx: &mut ModelContext<Self>) {}

    pub fn initiate_anonymous_user_linking(
        &self,
        _entrypoint: AnonymousUserSignupEntrypoint,
        _ctx: &mut ModelContext<Self>,
    ) {
    }

    /// Slim fork: callers used to optionally pass a custom token to the URL
    /// they open, but anonymous-user linking is dead now, so we just open the
    /// constructed URL without a token. Most callers in slim are themselves
    /// dead UI paths; this is the safe fallback.
    pub fn open_url_maybe_with_anonymous_token(
        &self,
        ctx: &mut ModelContext<Self>,
        construct_url: URLConstructorCallback,
    ) {
        let url: String = construct_url(None);
        if !url.is_empty() {
            ctx.open_url(&url);
        }
    }

    pub fn copy_anonymous_user_linking_url_to_clipboard(&self, _ctx: &mut ModelContext<Self>) {}

    /// Generates a unique state parameter for the authentication flow.
    /// Slim fork: kept only because tests still exercise the CSRF token
    /// machinery; production callers no longer fire it (URL builders are
    /// stubbed and `initialize_user_from_auth_payload` is a no-op).
    #[allow(dead_code)]
    fn generate_auth_state(&mut self) -> String {
        let state = Uuid::new_v4().to_string();
        self.pending_auth_state = Some(state.clone());
        state
    }

    // Slim fork: every URL below used to point at warp.dev (signup, login,
    // upgrade, SSO link). With no Warp account they have nowhere to go, so
    // each builder returns an empty string. Callers that still try to
    // `ctx.open_url(&...)` an empty URL get a no-op instead of a request to
    // the public Warp servers.

    pub fn sign_up_url(&mut self) -> String {
        String::new()
    }

    pub fn sign_in_url(&mut self) -> String {
        String::new()
    }

    pub fn upgrade_url(&mut self) -> String {
        String::new()
    }

    #[allow(dead_code)]
    pub fn login_options_url(&mut self, _custom_token: &str) -> String {
        String::new()
    }

    pub fn link_sso_url(&mut self, _email: &str) -> String {
        String::new()
    }

    /// Validates and consumes the pending auth state token. Returns `true` if the
    /// provided state matches; in that case the pending state is cleared so the
    /// CSRF token is single-use. A subsequent call with the same value will fail.
    #[allow(dead_code)]
    fn consume_auth_state(&mut self, received_state: &str) -> bool {
        if self.pending_auth_state.as_deref() == Some(received_state) {
            self.pending_auth_state = None;
            true
        } else {
            false
        }
    }

    /// Returns whether an auth redirect that failed state validation should be
    /// silently dropped rather than surfaced as an error. This covers the
    /// "user clicks the browser's 'Take me to Warp' button twice" case: once
    /// they're fully logged in, a second redirect targeting the same user is
    /// redundant and should not produce a user-visible error.
    #[allow(dead_code)]
    fn should_silently_ignore_stale_redirect(&self, incoming_user_uid: &Option<UserUid>) -> bool {
        if self.auth_state.is_anonymous_or_logged_out() {
            return false;
        }
        match (self.auth_state.user_id(), incoming_user_uid) {
            (Some(current_uid), Some(incoming_uid)) => current_uid == *incoming_uid,
            _ => false,
        }
    }

    /// Slim fork: only flip the local in-memory flag, no server round-trip.
    /// Persistence is also a no-op because slim never has Firebase creds.
    pub fn set_user_onboarded(&self, ctx: &mut ModelContext<Self>) {
        self.auth_state.set_is_onboarded(true);
        self.persist(ctx);
    }
}

#[derive(Clone, Debug)]
pub struct PersistedCurrentUserInformation {
    pub email: String,
}

impl Entity for AuthManager {
    type Event = AuthManagerEvent;
}

impl SingletonEntity for AuthManager {}

#[cfg(test)]
#[path = "auth_manager_test.rs"]
mod auth_manager_test;
