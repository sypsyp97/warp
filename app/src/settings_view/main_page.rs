//! Slim fork stub. The original "Account" page was the entry point for
//! sign-up / log-in / billing / referrals, all of which require a Warp
//! account. The page is dropped from the slim sidebar; this module
//! retains only the surface that other modules import:
//!
//!   * [`MainPageAction`] / [`MainSettingsPageEvent`] enums (referenced
//!     from `settings_view::SettingsAction` routing and event subscribers).
//!   * [`MainSettingsPageView`] struct that satisfies `SettingsPageMeta`
//!     so the page can still be registered for compile compat.
//!   * [`init_actions_from_parent_view`] / [`handle_experiment_change`]
//!     which set up the global "settings sync" toggle binding driven by
//!     the cloud-preferences feature flag. The flag is off in slim, so
//!     these end up no-ops too, but the symbols are imported externally.

use super::{
    settings_page::{MatchData, PageType, SettingsPageMeta, SettingsPageViewHandle},
    SettingsAction, SettingsSection, ToggleSettingActionPair,
};
use crate::auth::UserUid;
use crate::server::ids::ServerId;
use std::sync::{Arc, Mutex};
use warpui::{
    keymap::ContextPredicate, Action, AppContext, Element, Entity, TypedActionView, View,
    ViewContext, ViewHandle,
};

lazy_static::lazy_static! {
    static ref SETTINGS_SYNC_BINDINGS_ADDED: Arc<Mutex<bool>> = Default::default();
}

pub fn init_actions_from_parent_view<T: Action + Clone>(
    _app: &mut AppContext,
    _context: &ContextPredicate,
    _builder: fn(SettingsAction) -> T,
) {
    // Slim fork: no settings-sync binding to install. Mark the lock as
    // taken so a future feature-flag flip can't race on first install.
    if let Ok(mut lock) = SETTINGS_SYNC_BINDINGS_ADDED.lock() {
        *lock = true;
    }
    let _ = ToggleSettingActionPair::<T>::add_toggle_setting_action_pairs_as_bindings;
}

pub fn handle_experiment_change(_app: &mut AppContext) {
    // Slim fork: no cloud experiments to react to.
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum MainPageAction {
    Relaunch,
    DownloadUpdate,
    CheckForUpdate,
    ToggleSettingsSync,
    Upgrade {
        team_uid: Option<ServerId>,
        user_id: UserUid,
    },
    GenerateStripeBillingPortalLink {
        team_uid: ServerId,
    },
    SignupAnonymousUser,
    OpenUrl(String),
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum MainSettingsPageEvent {
    CheckForUpdate,
    OpenWarpDrive,
    SignupAnonymousUser,
}

pub struct MainSettingsPageView {
    page: PageType<Self>,
}

impl MainSettingsPageView {
    pub fn new(_ctx: &mut ViewContext<Self>) -> Self {
        Self {
            page: PageType::new_uncategorized(Vec::new(), Some("Account")),
        }
    }
}

impl Entity for MainSettingsPageView {
    type Event = MainSettingsPageEvent;
}

impl TypedActionView for MainSettingsPageView {
    type Action = MainPageAction;

    fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {
        // Slim fork: every action requires a Warp account; nothing to do.
    }
}

impl View for MainSettingsPageView {
    fn ui_name() -> &'static str {
        "MainSettingsPage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for MainSettingsPageView {
    fn section() -> SettingsSection {
        SettingsSection::Account
    }

    fn should_render(&self, _ctx: &AppContext) -> bool {
        false
    }

    fn update_filter(&mut self, query: &str, ctx: &mut ViewContext<Self>) -> MatchData {
        self.page.update_filter(query, ctx)
    }

    fn scroll_to_widget(&mut self, widget_id: &'static str) {
        self.page.scroll_to_widget(widget_id)
    }

    fn clear_highlighted_widget(&mut self) {
        self.page.clear_highlighted_widget();
    }
}

impl From<ViewHandle<MainSettingsPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<MainSettingsPageView>) -> Self {
        SettingsPageViewHandle::Main(view_handle)
    }
}
