//! Slim fork stub. The original platform page exposed Warp's
//! "Oz Cloud API Keys" — provisioning long-lived API tokens for the
//! Warp cloud. With no Warp cloud, the entire flow is dead.

use super::{
    settings_page::{MatchData, PageType, SettingsPageMeta, SettingsPageViewHandle},
    SettingsSection,
};
use warpui::{AppContext, Element, Entity, TypedActionView, View, ViewContext, ViewHandle};

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum PlatformPageViewEvent {
    ShowCreateApiKeyModal,
    HideCreateApiKeyModal,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum PlatformPageAction {
    ShowCreateApiKeyModal,
    HyperlinkClick(String),
}

pub struct PlatformPageView {
    page: PageType<Self>,
}

impl PlatformPageView {
    pub fn new(_ctx: &mut ViewContext<Self>) -> Self {
        Self {
            page: PageType::new_uncategorized(Vec::new(), None),
        }
    }

    pub fn get_modal_content(&self) -> Option<Box<dyn Element>> {
        None
    }
}

impl Entity for PlatformPageView {
    type Event = PlatformPageViewEvent;
}

impl TypedActionView for PlatformPageView {
    type Action = PlatformPageAction;

    fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {
        // Slim fork: API key provisioning requires the Warp cloud.
    }
}

impl View for PlatformPageView {
    fn ui_name() -> &'static str {
        "PlatformPage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for PlatformPageView {
    fn section() -> SettingsSection {
        SettingsSection::OzCloudAPIKeys
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

impl From<ViewHandle<PlatformPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<PlatformPageView>) -> Self {
        SettingsPageViewHandle::OzCloudAPIKeys(view_handle)
    }
}
