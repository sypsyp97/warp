//! Slim fork stub. The original `NeedsSsoLinkView` was the screen
//! shown when the user's org required them to link Warp's account to
//! an SSO provider. With no Warp account the screen is dead, but
//! `root_view` still owns a `ViewHandle<NeedsSsoLinkView>` and routes
//! the `AuthOnboardingState::NeedsSsoLink` state through it.

use warpui::elements::Empty;
use warpui::{AppContext, Element, Entity, TypedActionView, View, ViewContext};

#[derive(Debug)]
#[allow(dead_code)]
pub enum NeedsSsoLinkViewAction {
    ClickedLinkSsoButton,
}

pub struct NeedsSsoLinkView {
    _email: Option<String>,
}

impl NeedsSsoLinkView {
    pub fn new() -> Self {
        Self { _email: None }
    }

    pub fn set_email(&mut self, email: String) {
        self._email = Some(email);
    }
}

impl Entity for NeedsSsoLinkView {
    type Event = ();
}

impl View for NeedsSsoLinkView {
    fn ui_name() -> &'static str {
        "NeedsSsoLinkView"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        Empty::new().finish()
    }
}

impl TypedActionView for NeedsSsoLinkView {
    type Action = NeedsSsoLinkViewAction;

    fn handle_action(&mut self, _action: &NeedsSsoLinkViewAction, _ctx: &mut ViewContext<Self>) {
        // Slim fork: SSO link flow requires a Warp account.
    }
}
