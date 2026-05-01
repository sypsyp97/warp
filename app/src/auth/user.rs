use serde::{Deserialize, Serialize};
use warp_graphql::queries::get_user::FirebaseProfile;

use super::UserUid;

pub use warp_server_client::auth::{TEST_USER_EMAIL, TEST_USER_UID};

/// Type of principal making the authenticated request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrincipalType {
    #[default]
    User,
    ServiceAccount,
}

impl From<warp_graphql::queries::get_user::PrincipalType> for PrincipalType {
    fn from(value: warp_graphql::queries::get_user::PrincipalType) -> Self {
        use warp_graphql::queries::get_user::PrincipalType as GqlPrincipalType;
        match value {
            GqlPrincipalType::User => PrincipalType::User,
            GqlPrincipalType::ServiceAccount => PrincipalType::ServiceAccount,
        }
    }
}

/// The in-memory representation of a logged-in User.
/// This does not include authentication credentials, which are stored separately
/// in the `Credentials` enum.
#[derive(Debug, Clone)]
pub struct User {
    /// The Firebase UID of this user.
    pub local_id: UserUid,
    /// Metadata about the user.
    pub metadata: UserMetadata,
    /// Whether or not the user is onboarded.
    pub is_onboarded: bool,
    /// Type of principal (user or service account). Fetched fresh from the server
    /// on each login/refresh.
    pub principal_type: PrincipalType,
}

/// This struct holds extra information about the user. Most of this information comes directly
/// from Firebase.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UserMetadata {
    /// The user's email. NOTE: unlike other fields which use `Option`s to denote null values,
    /// an anonymous user will have an empty string as their email here.
    pub email: String,
    /// The user's display name from Firebase. We should prefer showing this over their email, if
    /// we can. Typically this is only populated when using a non-email provider like GitHub.
    pub display_name: Option<String>,
    /// A URL for their profile picture.
    pub photo_url: Option<String>,
}

impl User {
    /// The name for the user that we display. This is the user's display name, if set. If not set,
    /// we then fallback to email (which is always set).
    pub fn username_for_display(&self) -> &str {
        let user_metadata = &self.metadata;
        user_metadata
            .display_name
            .as_deref()
            .unwrap_or(user_metadata.email.as_str())
    }

    /// The display name of the user. Does not fall back to email.
    pub fn display_name(&self) -> Option<String> {
        self.metadata.display_name.clone()
    }

    #[allow(dead_code)]
    pub fn test() -> Self {
        Self {
            local_id: UserUid::new(TEST_USER_UID),
            metadata: UserMetadata {
                email: TEST_USER_EMAIL.to_string(),
                display_name: None,
                photo_url: None,
            },
            is_onboarded: true,
            principal_type: PrincipalType::User,
        }
    }
}

impl From<FirebaseProfile> for UserMetadata {
    fn from(value: FirebaseProfile) -> Self {
        Self {
            email: value.email.unwrap_or_default(),
            display_name: value.display_name,
            photo_url: value.photo_url,
        }
    }
}

#[cfg(test)]
#[path = "user_test.rs"]
mod tests;
