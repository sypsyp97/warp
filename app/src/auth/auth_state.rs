use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use parking_lot::RwLock;
use uuid::Uuid;
use warp_graphql::object_permissions::OwnerType;
use warpui::{AppContext, Entity, SingletonEntity};

use super::{
    anonymous_id::get_or_create_anonymous_id,
    credentials::Credentials,
    user::{PrincipalType, User},
    UserUid,
};

/// AuthState holds information about the currently-logged in user.
/// If you need to access AuthState, you can use the AuthStateProvider singleton model.
pub struct AuthState {
    /// The currently logged-in User. None if the user isn't logged in currently.
    user: RwLock<Option<User>>,

    /// An anonymous UUID. Can be used to consistently identify an anonymous user who is not logged in.
    anonymous_id: Uuid,

    /// State that indicates whether the current user's refresh token has been
    /// invalidated, meaning a reauth is required.
    needs_reauth: AtomicBool,

    /// The current authentication credentials.
    credentials: RwLock<Option<Credentials>>,
}

impl AuthState {
    fn new(ctx: &AppContext) -> Self {
        Self {
            user: RwLock::new(None),
            anonymous_id: get_or_create_anonymous_id(ctx),
            needs_reauth: AtomicBool::new(false),
            credentials: RwLock::new(None),
        }
    }

    #[cfg(any(test, feature = "integration_tests"))]
    pub fn new_for_test() -> Self {
        Self {
            user: RwLock::new(Some(User::test())),
            anonymous_id: Uuid::new_v4(),
            needs_reauth: AtomicBool::new(false),
            credentials: RwLock::new(Some(Credentials::Test)),
        }
    }

    /// Creates and initializes auth state.
    ///
    /// Slim fork: there is no Warp account. Leave credentials/user empty so
    /// every `is_logged_in()` branch in the codebase that gates a cloud call
    /// naturally short-circuits. AI gating uses a separate override in
    /// `AISettings::is_any_ai_enabled`.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub fn initialize(ctx: &AppContext, _api_key: Option<String>) -> Self {
        Self::new(ctx)
    }

    /// Sets the user. This should only be called by the AuthManager, to ensure
    /// side-effects are handled properly (e.g. notifying other models, persisting
    /// the user to secure storage, etc.).
    pub(super) fn set_user(&self, user: Option<User>) {
        *self.user.write() = user;
    }

    /// Returns the current credentials.
    pub fn credentials(&self) -> Option<Credentials> {
        self.credentials.read().clone()
    }

    /// Sets the credentials. Should only be called within the auth module.
    pub(super) fn set_credentials(&self, credentials: Option<Credentials>) {
        *self.credentials.write() = credentials;
    }

    /// Determines whether the user should be considered as logged in.
    pub fn is_logged_in(&self) -> bool {
        self.credentials.read().is_some()
    }

    /// Returns the cached access token, if any exists. This method *will not* check if the JWT is
    /// still valid! Usually, you want to use [`ServerApi::get_or_refresh_access_token`] instead!
    pub fn get_access_token_ignoring_validity(&self) -> Option<String> {
        let credentials = self.credentials.read();
        credentials.as_ref()?.bearer_token().bearer_token()
    }

    /// Returns the user's display name.
    pub fn username_for_display(&self) -> Option<String> {
        Some(self.user.read().as_ref()?.username_for_display().to_owned())
    }

    /// Returns the user's display name, does NOT fall back to email.
    pub fn display_name(&self) -> Option<String> {
        self.user
            .read()
            .as_ref()
            .and_then(|user| user.display_name().to_owned())
    }

    /// Returns the user's email. Note the non-obvious semantics of this function:
    /// If the user is logged in and not anonymous, the email will always be populated.
    /// If the user is logged in and anonymous, their email will be an empty string.
    /// If the user is not logged in, their email will be `None`.
    pub fn user_email(&self) -> Option<String> {
        self.user
            .read()
            .as_ref()
            .map(|user| user.metadata.email.clone())
    }

    /// Returns whether the user considered onboarded to Warp.
    pub fn is_onboarded(&self) -> Option<bool> {
        self.user.read().as_ref().map(|user| user.is_onboarded)
    }

    /// Set whether or not the user is onboarded.
    pub fn set_is_onboarded(&self, is_onboarded: bool) {
        if let Some(user) = self.user.write().as_mut() {
            user.is_onboarded = is_onboarded;
        }
    }

    /// If the user is logged in, returns their Firebase UID. Otherwise, returns None.
    pub fn user_id(&self) -> Option<UserUid> {
        self.user.read().as_ref().map(|user| user.local_id)
    }

    /// Returns the user's anonymous id.
    /// The anonymous id will be consistent across the app's lifetime. It is a random UUID.
    pub fn anonymous_id(&self) -> String {
        self.anonymous_id.to_string()
    }

    /// Returns whether a reauth is required for the current user given the state
    /// of their refresh token.
    pub fn needs_reauth(&self) -> bool {
        self.needs_reauth.load(Ordering::Relaxed)
    }

    /// Sets whether a reauth is required for the current user.
    /// Returns whether or not the reauth state was changed from false to true.
    pub(super) fn set_needs_reauth(&self, new_needs_reauth: bool) -> bool {
        let prev_needs_reauth = self.needs_reauth.swap(new_needs_reauth, Ordering::Relaxed);
        !prev_needs_reauth && new_needs_reauth
    }

    /// Returns whether the current user is authenticated via API key.
    pub fn is_api_key_authenticated(&self) -> bool {
        matches!(
            self.credentials.read().as_ref(),
            Some(Credentials::ApiKey { .. })
        )
    }

    /// Returns the API key if using API key authentication.
    pub fn api_key(&self) -> Option<String> {
        let credentials = self.credentials.read();
        credentials.as_ref()?.as_api_key().map(|s| s.to_owned())
    }

    /// Returns the type of principal (user or service account).
    pub fn principal_type(&self) -> Option<PrincipalType> {
        self.user.read().as_ref().map(|user| user.principal_type)
    }

    /// Returns whether the authenticated principal is a service account.
    pub fn is_service_account(&self) -> bool {
        matches!(self.principal_type(), Some(PrincipalType::ServiceAccount))
    }

    /// Returns the owner type of the currently-authenticated API key.
    pub fn api_key_owner_type(&self) -> Option<OwnerType> {
        self.credentials.read().as_ref()?.api_key_owner_type()
    }
}

// Adapter for the [`warp_managed_secrets`] crate, which needs to access the current user.
impl warp_managed_secrets::ActorProvider for AuthState {
    fn actor_uid(&self) -> Option<String> {
        self.user_id().map(|uid| uid.as_string())
    }
}

/// AuthStateProvider is a singleton model which provides a reference to the global AuthState.
pub struct AuthStateProvider {
    auth_state: Arc<AuthState>,
}

impl AuthStateProvider {
    pub fn new(auth_state: Arc<AuthState>) -> Self {
        Self { auth_state }
    }

    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self {
            auth_state: Arc::new(AuthState::new_for_test()),
        }
    }

    /// Constructs a provider backed by a fully logged-out `AuthState` (no user,
    /// no credentials). Used by unit tests that need to exercise code paths
    /// gated on `AuthState::user_id()` / `UserWorkspaces::personal_drive()`
    /// returning `None`.
    #[cfg(test)]
    pub fn new_logged_out_for_test() -> Self {
        Self {
            auth_state: Arc::new(AuthState {
                user: RwLock::new(None),
                anonymous_id: Uuid::new_v4(),
                needs_reauth: AtomicBool::new(false),
                credentials: RwLock::new(None),
            }),
        }
    }

    pub fn get(&self) -> &Arc<AuthState> {
        &self.auth_state
    }
}

impl Entity for AuthStateProvider {
    type Event = ();
}

impl SingletonEntity for AuthStateProvider {}
