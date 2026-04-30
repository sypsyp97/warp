//! Slim fork stub. The original "Billing and usage" page rendered the
//! user's Warp credit usage history and add-on purchase flow. With no
//! Warp account / billing, the entire page is dropped from the slim
//! sidebar. This module retains:
//!
//!   * [`create_discount_badge`] — also consumed by the buy-credits
//!     banner and the auto-reload modal (both dead in slim, but they
//!     still compile).
//!   * [`BillingAndUsagePageEvent`] / [`BillingAndUsagePageAction`] —
//!     referenced from `settings_view::SettingsAction` routing and
//!     event subscribers, plus from `From<&_> for LoginGatedFeature`.
//!   * The `BillingAndUsagePageView` struct + minimal trait impls.

use super::{
    settings_page::{MatchData, PageType, SettingsPageMeta, SettingsPageViewHandle},
    SettingsSection,
};
use crate::auth::auth_manager::LoginGatedFeature;
use crate::auth::UserUid;
use crate::server::ids::ServerId;
use crate::view_components::ToastFlavor;
use warp_core::ui::appearance::Appearance;
use warpui::{
    elements::{Element, Empty},
    AppContext, Entity, TypedActionView, View, ViewContext, ViewHandle,
};

pub fn create_discount_badge(_discount: u32, _appearance: &Appearance) -> Box<dyn Element> {
    // Slim fork: discount badges only appear in the (dead) buy-credits
    // banner and auto-reload modal. Always render nothing.
    Empty::new().finish()
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum BillingUsageTab {
    Overview,
    UsageHistory,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum BillingAndUsagePageEvent {
    SignupAnonymousUser,
    ShowToast {
        message: String,
        flavor: ToastFlavor,
    },
    ShowModal,
    HideModal,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
#[allow(dead_code)]
pub enum SortKey {
    DisplayName,
    Requests,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
#[allow(dead_code)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum BillingAndUsagePageAction {
    OpenUrl(String),
    Upgrade {
        team_uid: Option<ServerId>,
        user_id: UserUid,
    },
    GenerateStripeBillingPortalLink {
        team_uid: ServerId,
    },
    OpenAdminPanel {
        team_uid: ServerId,
    },
    ContactSupport,
    SignupAnonymousUser,
    AttemptLoginGatedUpgrade,
    UpdateUsageBasedPricingSettings {
        team_uid: ServerId,
        enabled: bool,
        max_monthly_spend_cents: Option<u32>,
    },
    ShowOverageLimitModal,
    RefreshWorkspaceData,
    ToggleSortingMenu,
    ChangeUsageSort {
        key: SortKey,
        order: SortOrder,
    },
    SelectTab(BillingUsageTab),
    ToggleUsageEntryExpanded {
        conversation_id: String,
    },
    RenderMoreUsageEntries,
    SelectTopupDenomination(usize),
    PurchaseAddonCredits {
        team_uid: ServerId,
    },
    ShowAddOnCreditModal,
    UpdateAutoReloadEnabled {
        team_uid: ServerId,
        enabled: bool,
    },
    DismissAmbientAgentTrialWidget,
    NavigateToByokSettings,
}

impl From<&BillingAndUsagePageAction> for LoginGatedFeature {
    fn from(val: &BillingAndUsagePageAction) -> LoginGatedFeature {
        use BillingAndUsagePageAction::*;
        match val {
            Upgrade { .. } => "Upgrade Plan",
            GenerateStripeBillingPortalLink { .. } => "Generate Stripe Billing Portal Link",
            _ => "Unknown reason",
        }
    }
}

pub struct BillingAndUsagePageView {
    page: PageType<Self>,
}

impl BillingAndUsagePageView {
    pub fn new(_ctx: &mut ViewContext<Self>) -> Self {
        Self {
            page: PageType::new_uncategorized(Vec::new(), None),
        }
    }

    pub fn get_modal_content(&self) -> Option<Box<dyn Element>> {
        None
    }
}

impl Entity for BillingAndUsagePageView {
    type Event = BillingAndUsagePageEvent;
}

impl TypedActionView for BillingAndUsagePageView {
    type Action = BillingAndUsagePageAction;

    fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {
        // Slim fork: every action requires Warp billing; nothing to do.
    }
}

impl View for BillingAndUsagePageView {
    fn ui_name() -> &'static str {
        "Billing and usage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for BillingAndUsagePageView {
    fn section() -> SettingsSection {
        SettingsSection::BillingAndUsage
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

impl From<ViewHandle<BillingAndUsagePageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<BillingAndUsagePageView>) -> Self {
        SettingsPageViewHandle::BillingAndUsage(view_handle)
    }
}
