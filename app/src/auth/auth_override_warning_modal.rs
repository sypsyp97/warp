//! Slim fork stub. The original `AuthOverrideWarningModal` was the
//! "you're already logged in — switch accounts?" guard around the auth
//! redirect flow. With no Warp account the flow is dead, but
//! `root_view` still constructs one and `workspace::view` calls
//! `set_interrupted_auth_payload`, so the public surface is preserved.

use crate::auth::auth_view_modal::AuthRedirectPayload;
use warpui::elements::Empty;
use warpui::{
    AppContext, Element, Entity, FocusContext, TypedActionView, View, ViewContext,
};

pub struct AuthOverrideWarningModal {
    _variant: AuthOverrideWarningModalVariant,
}

#[allow(dead_code)]
pub enum AuthOverrideWarningModalVariant {
    OnboardingView,
    WorkspaceModal,
}

impl AuthOverrideWarningModal {
    pub fn new(_ctx: &mut ViewContext<Self>, variant: AuthOverrideWarningModalVariant) -> Self {
        Self { _variant: variant }
    }

    pub fn set_interrupted_auth_payload(&mut self, _auth_payload: AuthRedirectPayload) {
        // Slim fork: there is no auth payload to resume.
    }
}

#[derive(PartialEq, Eq)]
#[allow(dead_code)]
pub enum AuthOverrideWarningModalEvent {
    Close,
    BulkExport,
}

impl Entity for AuthOverrideWarningModal {
    type Event = AuthOverrideWarningModalEvent;
}

impl View for AuthOverrideWarningModal {
    fn ui_name() -> &'static str {
        "AuthOverrideWarningModal"
    }

    fn on_focus(&mut self, _focus_ctx: &FocusContext, _ctx: &mut ViewContext<Self>) {}

    fn render(&self, _ctx: &AppContext) -> Box<dyn Element> {
        Empty::new().finish()
    }
}

impl TypedActionView for AuthOverrideWarningModal {
    type Action = ();
}
