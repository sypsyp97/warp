pub(super) mod user_persistence;

use std::sync::Arc;

use uuid::Uuid;
use warpui::{Entity, ModelContext, SingletonEntity};

use super::auth_state::{AuthState, PersistAction};
use super::auth_view_modal::{AuthRedirectPayload, AuthViewVariant};
use super::credentials::Credentials;
use super::user::User;
use super::AuthStateProvider;
use super::UserUid;
use crate::server::server_api::{
    auth::{AuthClient, MintCustomTokenError, UserAuthenticationError},
    ServerApi,
};
use crate::server::telemetry::AnonymousUserSignupEntrypoint;
use crate::{send_telemetry_from_ctx, TelemetryEvent};
use user_persistence::PersistedUser;

#[derive(Debug)]
pub enum AuthManagerEvent {
    /// Successfully authenticated a user with no errors.
    #[allow(dead_code)]
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
///
/// Slim fork: most callsites have been stubbed to no-ops, so the cached
/// `server_api` / `auth_client` handles are never invoked. They remain on
/// the struct so the public `new` signature stays stable for downstream
/// crates that construct it.
pub struct AuthManager {
    auth_state: Arc<AuthState>,
    #[allow(dead_code)]
    server_api: Arc<ServerApi>,
    #[allow(dead_code)]
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

    /// Slim fork: there are no Warp credentials to refresh. The CLI
    /// admin path still calls this on startup; we keep it as a quiet
    /// no-op so the call site doesn't need to know.
    pub fn refresh_user(&self, _ctx: &mut ModelContext<Self>) {}

    /// Slim fork: device auth flow is dead. CLI `warp login` sub-commands
    /// remain wired so the binary still type-checks, but they never
    /// produce credentials.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub fn authorize_device(&self, _ctx: &mut ModelContext<Self>) {}

    /// Sets the user and credentials in auth state and persists to secure storage.
    /// Persistence depends on the credential type — currently, only Firebase.
    /// Slim fork keeps this private helper for `log_out` to clear state.
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
