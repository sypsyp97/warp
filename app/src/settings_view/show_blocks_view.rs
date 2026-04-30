//! Slim fork stub. The original "Shared blocks" page listed every
//! shared session block on the user's Warp account; with no Warp
//! cloud, there's nothing to show. Reduced to an inert placeholder
//! that satisfies the trait surface consumed by `settings_view::mod`.

use std::sync::Arc;

use super::{
    settings_page::{MatchData, PageType, SettingsPageMeta, SettingsPageViewHandle},
    SettingsSection,
};
use crate::{server::server_api::block::BlockClient, view_components::ToastFlavor};
use warpui::{AppContext, Element, Entity, TypedActionView, View, ViewContext, ViewHandle};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ShowBlocksAction {
    CopyUrl(String),
    OverflowClick(usize),
    Unshare,
    ConfirmUnshare,
    CancelUnshare,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ShowBlocksEvent {
    ShowToast {
        message: String,
        flavor: ToastFlavor,
    },
}

pub struct ShowBlocksView {
    page: PageType<Self>,
}

impl ShowBlocksView {
    pub fn new(_block_client: Arc<dyn BlockClient>, _ctx: &mut ViewContext<Self>) -> Self {
        Self {
            page: PageType::new_uncategorized(Vec::new(), None),
        }
    }
}

impl Entity for ShowBlocksView {
    type Event = ShowBlocksEvent;
}

impl TypedActionView for ShowBlocksView {
    type Action = ShowBlocksAction;

    fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {
        // Slim fork: shared blocks require a Warp account; nothing to do.
    }
}

impl View for ShowBlocksView {
    fn ui_name() -> &'static str {
        "ShowBlocksView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for ShowBlocksView {
    fn section() -> SettingsSection {
        SettingsSection::SharedBlocks
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

impl From<ViewHandle<ShowBlocksView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<ShowBlocksView>) -> Self {
        SettingsPageViewHandle::SharedBlocks(view_handle)
    }
}
