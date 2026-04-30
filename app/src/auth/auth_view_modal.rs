//! Slim fork stub. The original `AuthView` was the modal that drove
//! Warp's sign-up / log-in flow. With no Warp account the modal is
//! permanently empty, so the view itself has been removed entirely.
//!
//! `AuthViewVariant` is preserved as a freestanding label because several
//! other modules still pass it as a parameter to
//! `AuthManager::attempt_login_gated_feature` (also a no-op now). Removing
//! the enum would force a much larger refactor with no behavioural benefit.

use warpui::AppContext;

pub fn init(_app: &mut AppContext) {
    // Slim fork: no key bindings to register; the auth modal is never shown.
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum AuthViewVariant {
    Initial,
    RequireLoginCloseable,
    HitDriveObjectLimitCloseable,
    ShareRequirementCloseable,
}
