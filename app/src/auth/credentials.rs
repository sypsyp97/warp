//! Slim fork: BYO API key is the only auth path. The `Firebase` /
//! `SessionCookie` Credentials variants and the `Firebase` / `NoAuth`
//! AuthToken variants are gone — they were never constructed once the
//! Warp cloud login was stripped, and removing them collapses the match
//! arms throughout the auth path.
use warp_graphql::object_permissions::OwnerType;

#[derive(Clone, Debug)]
pub enum Credentials {
    /// API key for direct server authentication.
    ApiKey {
        key: String,
        /// The owner type for this API key. Only set after user info is fetched from the server.
        owner_type: Option<OwnerType>,
    },
    /// Test credentials used in unit tests, integration tests, and skip_login builds.
    #[cfg(any(test, feature = "integration_tests", feature = "skip_login"))]
    Test,
}

impl Credentials {
    pub fn as_api_key(&self) -> Option<&str> {
        match self {
            Credentials::ApiKey { key, .. } => Some(key),
            #[cfg(any(test, feature = "integration_tests", feature = "skip_login"))]
            Credentials::Test => None,
        }
    }

    pub fn api_key_owner_type(&self) -> Option<OwnerType> {
        match self {
            Credentials::ApiKey { owner_type, .. } => *owner_type,
            #[cfg(any(test, feature = "integration_tests", feature = "skip_login"))]
            Credentials::Test => None,
        }
    }

    /// Returns the short-lived token to use in HTTP requests to the server.
    pub fn bearer_token(&self) -> AuthToken {
        match self {
            Credentials::ApiKey { key, .. } => AuthToken::ApiKey(key.clone()),
            #[cfg(any(test, feature = "integration_tests", feature = "skip_login"))]
            Credentials::Test => AuthToken::ApiKey(String::new()),
        }
    }
}

/// Represents different types of authentication tokens.
#[derive(Debug, Clone)]
pub enum AuthToken {
    /// API key for direct server authentication.
    ApiKey(String),
}

impl AuthToken {
    /// Returns the token string to use in an Authorization header.
    pub fn as_bearer_token(&self) -> Option<&str> {
        match self {
            AuthToken::ApiKey(key) => Some(key),
        }
    }

    /// Returns the bearer token as an owned string.
    pub fn bearer_token(&self) -> Option<String> {
        match self {
            AuthToken::ApiKey(key) => Some(key.clone()),
        }
    }
}
