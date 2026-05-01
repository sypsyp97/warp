//! Slim fork stub. The original referrals page lived behind a Warp
//! account so users could send invite links + earn credits; with no
//! Warp cloud, the entire flow is dead. Reduced to an inert
//! placeholder that satisfies the [`SettingsPageMeta`] /
//! [`TypedActionView`] surface consumed by `settings_view::mod`.

use std::sync::Arc;

use super::{
    settings_page::{MatchData, PageType, SettingsPageMeta, SettingsPageViewHandle},
    SettingsSection,
};
use crate::{server::server_api::referral::ReferralsClient, view_components::ToastFlavor};
use warpui::{AppContext, Element, Entity, TypedActionView, View, ViewContext, ViewHandle};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ReferralsPageAction {
    CopyLink,
    SendEmailInvite,
}

#[allow(dead_code)]
pub enum ReferralsPageEvent {
    FocusModal,
    ShowToast {
        message: String,
        flavor: ToastFlavor,
    },
}

pub struct ReferralsPageView {
    page: PageType<Self>,
}

impl ReferralsPageView {
    pub fn new(
        _referrals_client: Arc<dyn ReferralsClient>,
        _ctx: &mut ViewContext<Self>,
    ) -> Self {
        Self {
            page: PageType::new_uncategorized(Vec::new(), None),
        }
    }
}

impl Entity for ReferralsPageView {
    type Event = ReferralsPageEvent;
}

impl TypedActionView for ReferralsPageView {
    type Action = ReferralsPageAction;

    fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {
        // Slim fork: every action requires a Warp account; nothing to do.
    }
}

impl View for ReferralsPageView {
    fn ui_name() -> &'static str {
        "ReferralsPage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for ReferralsPageView {
    fn section() -> SettingsSection {
        SettingsSection::Referrals
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

impl From<ViewHandle<ReferralsPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<ReferralsPageView>) -> Self {
        SettingsPageViewHandle::Referrals(view_handle)
    }
}
