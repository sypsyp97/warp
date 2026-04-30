use warpui::AppContext;
use warpui_extras::secure_storage::{self, AppContextExt};

const USER_STORAGE_KEY: &str = "User";

#[derive(Debug, thiserror::Error)]
pub enum UserPersistenceError {
    #[error("secure storage error")]
    SecureStorageError(#[from] secure_storage::Error),
}

/// Slim fork: only `remove_from_secure_storage` survives — there is no Warp
/// account to read or write, so the serde'd struct that used to live here
/// (Firebase tokens, anonymous-user limits, SSO flags, etc.) is gone.
/// Logout still clears the legacy key in case an older build wrote one.
pub struct PersistedUser;

impl PersistedUser {
    pub fn remove_from_secure_storage(ctx: &AppContext) -> Result<(), UserPersistenceError> {
        Ok(ctx.secure_storage().remove_value(USER_STORAGE_KEY)?)
    }
}
