//! Slim fork stub. The original `PasteAuthTokenModalView` was the
//! "paste your token from the browser" modal that ran the
//! `AuthRedirectPayload` parser on the contents and routed the result
//! through `AuthManager::initialize_user_from_auth_payload`. With no
//! Warp account the modal is dead, but `root_view` still holds an
//! `Option<ViewHandle<PasteAuthTokenModalView>>` so the public surface
//! is preserved here.

use warpui::elements::Empty;
use warpui::{
    AppContext, Element, Entity, FocusContext, TypedActionView, View, ViewContext,
};

pub fn init(_app: &mut AppContext) {
    // Slim fork: no key bindings to register; the modal is never shown.
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum PasteAuthTokenModalAction {
    Confirm,
    Cancel,
    PasteIntoEditor,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum PasteAuthTokenModalEvent {
    Cancelled,
}

pub struct PasteAuthTokenModalView;

impl PasteAuthTokenModalView {
    pub fn new(_ctx: &mut ViewContext<Self>) -> Self {
        Self
    }
}

impl Entity for PasteAuthTokenModalView {
    type Event = PasteAuthTokenModalEvent;
}

impl View for PasteAuthTokenModalView {
    fn ui_name() -> &'static str {
        "PasteAuthTokenModalView"
    }

    fn on_focus(&mut self, _focus_ctx: &FocusContext, _ctx: &mut ViewContext<Self>) {}

    fn render(&self, _ctx: &AppContext) -> Box<dyn Element> {
        Empty::new().finish()
    }
}

impl TypedActionView for PasteAuthTokenModalView {
    type Action = PasteAuthTokenModalAction;

    fn handle_action(&mut self, _action: &PasteAuthTokenModalAction, _ctx: &mut ViewContext<Self>) {
        // Slim fork: every action requires the auth flow.
    }
}
