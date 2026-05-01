pub(super) mod user_persistence;

use std::sync::Arc;

use warpui::{Entity, ModelContext, SingletonEntity};

use super::auth_state::AuthState;
use super::AuthStateProvider;
use crate::{send_telemetry_from_ctx, TelemetryEvent};
use user_persistence::PersistedUser;

#[derive(Debug)]
pub enum AuthManagerEvent {
    /// The user now needs to reauthenticate. Emitted from code paths where we
    /// don't refresh the entire user, only their token.
    NeedsReauth,
}

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

    /// Slim fork: there are no Warp credentials to refresh. App startup and
    /// the CLI agent loop still call this; keep it as a quiet no-op so the
    /// call sites don't need to special-case slim.
    pub fn refresh_user(&self, _ctx: &mut ModelContext<Self>) {}

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
