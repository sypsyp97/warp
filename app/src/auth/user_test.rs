use super::*;
use anyhow::Result;
use warp_graphql::queries::get_user::FirebaseProfile;

#[test]
fn test_parse_user_profile() -> Result<()> {
    let response: FirebaseProfile = serde_json::from_str(
        r#"{
            "uid": "test_local_id",
            "email": "test_user@example.com",
            "displayName": "Test User",
            "photoUrl": "https://photourl.example.com/1234",
            "needsSsoLink": true
        }"#,
    )?;
    let user = User {
        is_onboarded: true,
        local_id: UserUid::new("test_local_id"),
        metadata: response.into(),
        principal_type: PrincipalType::User,
    };
    assert_eq!(user.metadata.display_name.as_deref(), Some("Test User"));
    assert_eq!(user.metadata.email, "test_user@example.com");
    assert_eq!(
        user.metadata.photo_url.as_deref(),
        Some("https://photourl.example.com/1234")
    );

    Ok(())
}
