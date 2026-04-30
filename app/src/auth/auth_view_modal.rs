//! Slim fork stub. The original `AuthView` was the modal that drove
//! Warp's sign-up / log-in flow (browser handoff, paste-the-token,
//! anonymous-user fallback, login failure notification, etc.). With no
//! Warp account, the entire flow is dead — but several other modules
//! still hold a `ViewHandle<AuthView>` and reference its surface:
//!
//!   * `root_view.rs` keeps an `auth_view` member (created at startup)
//!     and pokes `last_login_failure_reason` / `set_variant`.
//!   * `workspace/view.rs` keeps a `require_login_modal: ViewHandle<AuthView>`,
//!     subscribes to `AuthViewEvent::Close`, and calls `set_variant` /
//!     `skip_to_browser_open_step`.
//!   * `ai_page`, `drive/index`, `terminal/view`, `pane_group`,
//!     `command_search` reference `AuthViewVariant` to label
//!     login-gated calls (the calls themselves are now no-ops).
//!   * `uri::mod` parses `AuthRedirectPayload` from incoming URLs.
//!
//! This stub keeps the public surface intact and renders nothing.

use crate::auth::UserUid;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use url::Url;
use warpui::elements::Empty;
use warpui::{
    AppContext, Element, Entity, TypedActionView, View, ViewContext,
};

pub fn init(_app: &mut AppContext) {
    // Slim fork: no key bindings to register; the auth modal is never shown.
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum AuthViewAction {
    PasteAuthUrl,
    DismissErrorNotification,
}

const AUTH_URL_HOST: &str = "auth";
const AUTH_URL_REFRESH_TOKEN_QUERY_PARAM: &str = "refresh_token";
const AUTH_URL_NEW_USER_UID_QUERY_PARAM: &str = "user_uid";
const AUTH_URL_DELETED_ANON_USER_QUERY_PARAM: &str = "deleted_anonymous_user";
const AUTH_URL_STATE_QUERY_PARAM: &str = "state";

/// Returned from the incoming redirect URL. Slim never receives one in
/// practice but the parser is still wired up to `uri::mod` so the URL
/// format stays valid in case a stale link makes its way in.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AuthRedirectPayload {
    pub refresh_token: String,
    pub user_uid: Option<UserUid>,
    pub deleted_anonymous_user: Option<bool>,
    pub state: Option<String>,
}

impl AuthRedirectPayload {
    pub fn from_url(url: Url) -> Result<Self> {
        if url.host_str() != Some(AUTH_URL_HOST) {
            return Err(anyhow!("Received URL with unexpected host: {} ", url));
        }
        let query_params: HashMap<_, _> = url.query_pairs().into_owned().collect();
        if let Some(token) = query_params.get(AUTH_URL_REFRESH_TOKEN_QUERY_PARAM) {
            let user_uid = query_params
                .get(AUTH_URL_NEW_USER_UID_QUERY_PARAM)
                .map(|uid| UserUid::new(uid));

            Ok(Self {
                refresh_token: token.to_string(),
                user_uid,
                deleted_anonymous_user: query_params
                    .get(AUTH_URL_DELETED_ANON_USER_QUERY_PARAM)
                    .map(|value| value == "true"),
                state: query_params.get(AUTH_URL_STATE_QUERY_PARAM).cloned(),
            })
        } else {
            Err(anyhow!(
                "Received URL without refresh token query param: {}",
                url
            ))
        }
    }

    #[allow(dead_code)]
    pub fn from_raw_url(raw_url: String) -> Result<Self> {
        match Url::parse(&raw_url) {
            Ok(parsed_url) => AuthRedirectPayload::from_url(parsed_url),
            Err(error) => Err(anyhow!(error)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum AuthViewVariant {
    Initial,
    RequireLoginCloseable,
    HitDriveObjectLimitCloseable,
    ShareRequirementCloseable,
}

pub struct AuthView {
    _variant: AuthViewVariant,
}

impl AuthView {
    pub fn new(variant: AuthViewVariant, _ctx: &mut ViewContext<Self>) -> Self {
        Self { _variant: variant }
    }

    pub fn set_variant(&mut self, _ctx: &mut ViewContext<Self>, variant: AuthViewVariant) {
        self._variant = variant;
    }

    pub fn skip_to_browser_open_step(&mut self, _ctx: &mut ViewContext<Self>) {
        // Slim fork: there is no browser-open step.
    }
}

#[derive(PartialEq, Eq)]
#[allow(dead_code)]
pub enum AuthViewEvent {
    Close,
}

impl Entity for AuthView {
    type Event = AuthViewEvent;
}

impl View for AuthView {
    fn ui_name() -> &'static str {
        "AuthView"
    }

    fn render(&self, _ctx: &AppContext) -> Box<dyn Element> {
        Empty::new().finish()
    }
}

impl TypedActionView for AuthView {
    type Action = AuthViewAction;

    fn handle_action(&mut self, _action: &AuthViewAction, _ctx: &mut ViewContext<Self>) {
        // Slim fork: every action requires the auth flow.
    }
}
