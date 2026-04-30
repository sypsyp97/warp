//! Slim fork stub. The original teams page handled invites, billing,
//! domain restrictions, role management, and discoverability — all of
//! which require Warp's cloud workspace service. With no cloud, the
//! page is dropped from the slim sidebar. This module retains only:
//!
//!   * [`TeamsPageView`] struct with `new` and `open_team_members` —
//!     the latter is called from `mod.rs::open_teams_page_email_invite`.
//!   * [`TeamsPageViewEvent`] — referenced from event subscribers.
//!   * [`OpenTeamsSettingsModalArgs`] — referenced from `root_view`
//!     and the `uri` parser.
//!   * [`TeamsInviteOption`] — referenced from telemetry events.
//!   * [`TeamsPageAction`] — drives `TypedActionView`. Not dispatched.

use super::{
    settings_page::{MatchData, PageType, SettingsPageMeta, SettingsPageViewHandle},
    SettingsSection,
};
use crate::auth::auth_manager::LoginGatedFeature;
use crate::auth::UserUid;
use crate::server::ids::ServerId;
use crate::server::telemetry::TelemetryEvent;
use crate::view_components::ToastFlavor;
use crate::workspaces::team::MembershipRole;
use serde::{Deserialize, Serialize};
use warpui::{AppContext, Element, Entity, TypedActionView, View, ViewContext, ViewHandle};

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum TeamsPageAction {
    LeaveTeam,
    ShowLeaveTeamConfirmationDialog,
    ShowDeleteTeamConfirmationDialog,
    CopyLink(String),
    CreateTeam,
    ChangeInviteViewOption(TeamsInviteOption),
    DeletePendingEmailInvitation {
        team_uid: ServerId,
        invitee_email: String,
    },
    RemoveUserFromTeam {
        user_uid: UserUid,
        team_uid: ServerId,
    },
    ToggleIsInviteLinkEnabled {
        team_uid: ServerId,
        current_state: bool,
    },
    ResetInviteLinks {
        team_uid: ServerId,
    },
    AddDomainRestrictions {
        team_uid: ServerId,
    },
    DeleteDomainRestriction {
        domain_uid: ServerId,
        team_uid: ServerId,
    },
    SendEmailInvites {
        team_uid: ServerId,
    },
    OpenWarpDrive,
    GenerateUpgradeLink {
        team_uid: ServerId,
    },
    GenerateStripeBillingPortalLink {
        team_uid: ServerId,
    },
    OpenAdminPanel {
        team_uid: ServerId,
    },
    ContactSupport,
    ToggleTeamDiscoverabilityBeforeCreation,
    ToggleTeamDiscoverability {
        team_uid: ServerId,
        current_state: bool,
    },
    JoinTeamWithTeamDiscovery {
        team_uid: ServerId,
    },
    ShowTransferOwnershipModal {
        new_owner_email: String,
        new_owner_uid: UserUid,
        team_uid: ServerId,
    },
    OpenMemberActionsMenu {
        index: usize,
    },
    CloseMemberActionsMenu,
    SetTeamMemberRole {
        team_uid: ServerId,
        user_uid: UserUid,
        role: MembershipRole,
    },
}

impl From<&TeamsPageAction> for LoginGatedFeature {
    fn from(_val: &TeamsPageAction) -> LoginGatedFeature {
        // Slim fork: no actions are gated because none can be triggered.
        "Unknown reason"
    }
}

impl TryFrom<&TeamsPageAction> for TelemetryEvent {
    type Error = anyhow::Error;
    fn try_from(_action: &TeamsPageAction) -> Result<Self, Self::Error> {
        Err(anyhow::anyhow!(
            "We do not log this telemetry event from the client."
        ))
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum TeamsPageViewEvent {
    TeamsChanged,
    OpenWarpDrive,
    ShowToast {
        message: String,
        flavor: ToastFlavor,
    },
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Copy, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum TeamsInviteOption {
    #[default]
    Link,
    Email,
}

impl std::fmt::Display for TeamsInviteOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TeamsInviteOption::Link => "Link",
                TeamsInviteOption::Email => "Email",
            },
        )
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OpenTeamsSettingsModalArgs {
    pub invite_email: Option<String>,
}

pub struct TeamsPageView {
    page: PageType<Self>,
}

impl TeamsPageView {
    pub fn new(_ctx: &mut ViewContext<Self>) -> Self {
        Self {
            page: PageType::new_uncategorized(Vec::new(), None),
        }
    }

    pub fn open_team_members(&mut self, _email: Option<&String>, _ctx: &mut ViewContext<Self>) {
        // Slim fork: no team management surface.
    }
}

impl Entity for TeamsPageView {
    type Event = TeamsPageViewEvent;
}

impl TypedActionView for TeamsPageView {
    type Action = TeamsPageAction;

    fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {
        // Slim fork: every action requires the Warp cloud workspace service.
    }
}

impl View for TeamsPageView {
    fn ui_name() -> &'static str {
        "TeamsPage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for TeamsPageView {
    fn section() -> SettingsSection {
        SettingsSection::Teams
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

impl From<ViewHandle<TeamsPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<TeamsPageView>) -> Self {
        SettingsPageViewHandle::Teams(view_handle)
    }
}
