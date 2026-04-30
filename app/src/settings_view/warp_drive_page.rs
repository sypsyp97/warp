//! Slim fork stub. The real Warp Drive settings page lived behind the
//! `OpenWarpNewSettingsModes` feature flag and a Warp account; neither
//! exists in slim, so the page is reduced to an inert placeholder that
//! satisfies the [`SettingsPageMeta`] / [`TypedActionView`] surface
//! consumed by `settings_view::mod`.

use super::{
    settings_page::{MatchData, PageType, SettingsPageMeta, SettingsPageViewHandle},
    SettingsSection,
};
use warpui::{AppContext, Element, Entity, TypedActionView, View, ViewContext, ViewHandle};

#[derive(Debug, Clone)]
pub enum WarpDriveSettingsPageAction {
    ToggleShowWarpDrive,
    SignUp,
    OpenUrl(String),
}

pub enum WarpDriveSettingsPageEvent {
    #[allow(dead_code)]
    SignUp,
}

pub struct WarpDriveSettingsPageView {
    page: PageType<Self>,
}

impl WarpDriveSettingsPageView {
    pub fn new(_ctx: &mut ViewContext<Self>) -> Self {
        Self {
            page: PageType::new_uncategorized(Vec::new(), None),
        }
    }
}

impl Entity for WarpDriveSettingsPageView {
    type Event = WarpDriveSettingsPageEvent;
}

impl TypedActionView for WarpDriveSettingsPageView {
    type Action = WarpDriveSettingsPageAction;

    fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {
        // Slim fork: every action requires a Warp account; nothing to do.
    }
}

impl View for WarpDriveSettingsPageView {
    fn ui_name() -> &'static str {
        "WarpDrivePage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for WarpDriveSettingsPageView {
    fn section() -> SettingsSection {
        SettingsSection::WarpDrive
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

impl From<ViewHandle<WarpDriveSettingsPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<WarpDriveSettingsPageView>) -> Self {
        SettingsPageViewHandle::WarpDrive(view_handle)
    }
}
