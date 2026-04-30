//! Slim fork stub. The original `LoginSlideView` was the onboarding
//! slide that ran the user through Warp's "select auth pathway →
//! browser open → privacy settings" flow. With no Warp account the
//! whole flow is dead, but `root_view` still constructs a slide on the
//! "log in" link so the public surface is preserved here.

use onboarding::OnboardingIntention;
use warpui::elements::Empty;
use warpui::{
    AppContext, Element, Entity, FocusContext, TypedActionView, View, ViewContext,
};

pub fn init(_app: &mut AppContext) {
    // Slim fork: no key bindings to register; the slide is never shown.
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum LoginSlideAction {
    Enter,
    ShowSkipDialog,
    ConfirmSkip,
    DismissDialog,
    DismissOverlayOrBack,
    Back,
    BackToSelectAuthPathway,
    CopyLoginUrl,
    EnterToken,
    ShowPrivacySettings,
    HideOverlay,
    ToggleTelemetry,
    ToggleCrashReporting,
    ToggleCloudConversationStorage,
    DismissNotification,
    PasteAuthUrl,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum LoginSlideEvent {
    BackToOnboarding,
    LoginLaterConfirmed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum LoginSlideSource {
    OnboardingFlow,
    LoginExistingUserFromWelcome,
    PrivacySettingsFromTerminalIntentionTheme,
}

pub struct LoginSlideView {
    _source: LoginSlideSource,
}

impl LoginSlideView {
    pub fn new(
        _ai_enabled: bool,
        _theme_name: &str,
        _use_vertical_tabs: bool,
        _intention: OnboardingIntention,
        source: LoginSlideSource,
        _ctx: &mut ViewContext<Self>,
    ) -> Self {
        Self { _source: source }
    }

    /// Slim fork: the auth-token input is never visible (the slide is dead).
    pub fn is_auth_token_input_visible(&self) -> bool {
        false
    }
}

impl Entity for LoginSlideView {
    type Event = LoginSlideEvent;
}

impl View for LoginSlideView {
    fn ui_name() -> &'static str {
        "LoginSlideView"
    }

    fn on_focus(&mut self, _focus_ctx: &FocusContext, _ctx: &mut ViewContext<Self>) {}

    fn render(&self, _ctx: &AppContext) -> Box<dyn Element> {
        Empty::new().finish()
    }
}

impl TypedActionView for LoginSlideView {
    type Action = LoginSlideAction;

    fn handle_action(&mut self, _action: &LoginSlideAction, _ctx: &mut ViewContext<Self>) {
        // Slim fork: every login action is gated on the Warp account flow.
    }
}
