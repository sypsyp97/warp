//! Slim fork stub. The original module rendered a dismissable error
//! notification overlay when login failed. Slim never shows the auth
//! flow, but `root_view::handle_paste` (and a few other call sites)
//! still set `last_login_failure_reason = Some(...)` on the dead
//! `AuthView`, so the enum is preserved.

#[allow(dead_code)]
pub enum LoginFailureReason {
    InvalidRedirectUrl { was_pasted: bool },
    FailedUserAuthentication,
    FailedMintCustomToken,
    InvalidStateParameter,
    MissingStateParameter,
}
