//! Slim fork stub. The original `AuthView` was the modal that drove
//! Warp's sign-up / log-in flow. With no Warp account the modal is
//! permanently empty, but several modules still hold a `ViewHandle<AuthView>`
//! and call `set_variant` / `skip_to_browser_open_step` on it, and several
//! more pass `AuthViewVariant` as a label to `attempt_login_gated_feature`
//! (also a no-op now). The minimum public surface to keep them all
//! compiling is preserved here.

use warpui::elements::Empty;
use warpui::{AppContext, Element, Entity, TypedActionView, View, ViewContext};

pub fn init(_app: &mut AppContext) {
    // Slim fork: no key bindings to register; the auth modal is never shown.
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum AuthViewAction {
    PasteAuthUrl,
    DismissErrorNotification,
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
