pub(super) mod user_persistence;

use std::sync::Arc;

use warpui::{Entity, ModelContext, SingletonEntity};

use super::auth_state::AuthState;
use super::auth_view_modal::AuthViewVariant;
use super::AuthStateProvider;
use crate::server::server_api::auth::{MintCustomTokenError, UserAuthenticationError};
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
    #[allow(dead_code)]
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

/// AuthManager is a singleton model which manages the currently logged-in user's state.
/// If you need to access the state, use `AuthStateProvider`.
///
/// Slim fork: every method that used to talk to Warp's cloud is a no-op,
/// so the manager only carries the AuthState handle. The cloud client
/// references that the upstream version held are gone.
pub struct AuthManager {
    auth_state: Arc<AuthState>,
}

impl AuthManager {
    /// Creates a new instance of the AuthManager. The auth state must already be initialized through
    /// [`AuthStateProvider`].
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        Self {
            auth_state: AuthStateProvider::as_ref(ctx).get().clone(),
        }
    }

    #[cfg(test)]
    pub fn new_for_test(ctx: &mut ModelContext<Self>) -> Self {
        Self::new(ctx)
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

    /// Helper function for logging out the user.
    /// NOTE: You probably want to call auth::log_out instead; this only manages the auth state,
    /// it doesn't shut down any other user-dependent parts of the app.
    pub(super) fn log_out(&mut self, ctx: &mut ModelContext<Self>) {
        self.auth_state.set_user(None);
        self.auth_state.set_credentials(None);
        let _ = PersistedUser::remove_from_secure_storage(ctx).map_err(|err| {
            log::warn!("Unable to clear user from secure storage: {err:?}");
        });
    }

    /// Sets whether or not this user's Firebase credentials are invalid and thus needs to reauth.
    pub fn set_needs_reauth(&self, needs_reauth: bool, ctx: &mut ModelContext<Self>) {
        let became_true = self.auth_state.set_needs_reauth(needs_reauth);

        if became_true {
            send_telemetry_from_ctx!(TelemetryEvent::NeedsReauth, ctx);
            ctx.emit(AuthManagerEvent::NeedsReauth);
        }
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

    /// Slim fork: only flip the local in-memory flag, no server round-trip.
    /// No-op when there's no user (the common case in slim).
    pub fn set_user_onboarded(&self, _ctx: &mut ModelContext<Self>) {
        self.auth_state.set_is_onboarded(true);
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
