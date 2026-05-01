//! Stubbed-out autoupdate surface.
//!
//! The slim fork does not talk to Warp's release servers, download installers,
//! or relaunch into a new version. This module keeps every public type and
//! function signature the rest of the crate references so the ~30 external
//! call sites continue to compile, but every body is a no-op:
//!
//! * `get_update_state` always reports `AutoupdateStage::NoUpdateAvailable`.
//! * `apply_update` returns `Ok(ReadyForRelaunch::No)` — callers match on
//!   `ReadyForRelaunch::Yes`, so the relaunch path is short-circuited.
//! * `is_incoming_version_past_current` always returns `false`.
//! * `AutoupdateState::manually_check_for_update` /
//!   `maybe_daily_check_for_update` do nothing; no events are ever emitted.
//! * The `linux::UpdateMethod::detect()` shim still exists for `debug_dump.rs`.
//!
//! All platform-specific logic (`linux.rs`, `mac.rs`, `windows.rs`), the
//! changelog fetcher, and the channel-versions fetcher have been deleted.

// The whole module is intentionally a stub; many of its types and fields
// are still part of the public surface that the rest of the crate calls
// into, but never observed in the slim fork. Silence the noise.
#![allow(dead_code)]

use std::sync::Arc;

use anyhow::Result;
use channel_versions::{Changelog, VersionInfo};
use warpui::accessibility::AccessibilityContent;
use warpui::{AppContext, Entity, ModelContext, SingletonEntity, ViewContext};

use crate::server::server_api::ServerApi;
use crate::workspace::Workspace;

/// Inline stub for `crate::autoupdate::linux::UpdateMethod::detect()`, which is
/// still referenced from `app/src/debug_dump.rs` on Linux.
#[cfg(target_os = "linux")]
pub mod linux {
    #[derive(Debug, Clone)]
    pub struct UpdateMethod;

    impl UpdateMethod {
        pub fn detect() -> Self {
            Self
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum AutoupdateStage {
    #[default]
    NoUpdateAvailable,
    CheckingForUpdate,
    DownloadingUpdate,
    UnableToUpdateToNewVersion {
        new_version: VersionInfo,
    },
    UpdateReady {
        new_version: VersionInfo,
        update_id: String,
    },
    Updating {
        new_version: VersionInfo,
        update_id: String,
    },
    UnableToLaunchNewVersion {
        new_version: VersionInfo,
    },
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    UpdatedPendingRestart {
        new_version: VersionInfo,
    },
}

impl AutoupdateStage {
    pub fn ready_for_update(&self) -> bool {
        matches!(
            self,
            AutoupdateStage::UpdateReady { .. } | AutoupdateStage::UpdatedPendingRestart { .. }
        )
    }

    pub fn available_new_version(&self) -> Option<&VersionInfo> {
        match self {
            AutoupdateStage::UpdateReady { new_version, .. }
            | AutoupdateStage::Updating { new_version, .. }
            | AutoupdateStage::UpdatedPendingRestart { new_version }
            | AutoupdateStage::UnableToLaunchNewVersion { new_version }
            | AutoupdateStage::UnableToUpdateToNewVersion { new_version } => Some(new_version),
            _ => None,
        }
    }
}

#[derive(Default)]
pub struct AutoupdateState {
    stage: AutoupdateStage,
    #[allow(dead_code)]
    server_api: Option<Arc<ServerApi>>,
}

impl AutoupdateState {
    pub fn new(server_api: Arc<ServerApi>) -> Self {
        Self {
            stage: AutoupdateStage::NoUpdateAvailable,
            server_api: Some(server_api),
        }
    }

    pub fn register(ctx: &mut AppContext, server_api: Arc<ServerApi>) {
        ctx.add_singleton_model(move |_ctx| Self::new(server_api));
    }

    /// Stub: the slim fork never checks for updates.
    pub fn manually_check_for_update(&mut self, _ctx: &mut ModelContext<Self>) {}

    /// Stub: the slim fork never checks for updates.
    pub fn maybe_daily_check_for_update(&mut self, _ctx: &mut ModelContext<Self>) {}
}

/// The set of events that are emitted from the AutoupdateState model.
#[allow(dead_code)]
pub enum AutoupdateStateEvent {
    CheckComplete {
        result: Result<UpdateReady>,
        request_type: RequestType,
    },
    UpdateAvailable,
}

impl Entity for AutoupdateState {
    type Event = AutoupdateStateEvent;
}

impl SingletonEntity for AutoupdateState {}

/// Set of results from an update check.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateReady {
    Yes {
        new_version: VersionInfo,
        update_id: String,
    },
    CanDownload {
        new_version: VersionInfo,
        update_id: String,
    },
    No,
}

/// Whether or not we're ready to relaunch the app after the user requests that
/// we apply an update.
#[allow(dead_code)]
pub enum ReadyForRelaunch {
    Yes,
    No,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum RequestType {
    ManualCheck,
    Poll,
    DailyCheck,
}

/// Stub: never produces accessibility content because we never check for updates.
pub fn accessibility_content(
    _update_available: &Result<UpdateReady>,
    _request_type: RequestType,
) -> Option<AccessibilityContent> {
    None
}

pub fn get_update_state(_app: &AppContext) -> AutoupdateStage {
    AutoupdateStage::NoUpdateAvailable
}

/// Stub: returns `Ok(ReadyForRelaunch::No)` so callers short-circuit the
/// relaunch path (workspace/view.rs:15302 matches on `ReadyForRelaunch::Yes`).
pub fn apply_update(
    _initiating_workspace: &mut Workspace,
    _ctx: &mut ViewContext<Workspace>,
) -> Result<ReadyForRelaunch> {
    Ok(ReadyForRelaunch::No)
}

/// Stub: no relaunch ever happens.
pub fn initiate_relaunch_for_update(_app: &mut AppContext) {}

pub fn manually_download_new_version(_ctx: &mut AppContext) {}

#[derive(Clone, Copy, Default)]
pub struct RelaunchModel;

impl RelaunchModel {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Entity for RelaunchModel {
    type Event = ();
}

impl SingletonEntity for RelaunchModel {}

pub fn is_incoming_version_past_current(_version: Option<&str>) -> bool {
    false
}

/// Stub: the slim fork does not fetch changelogs from Warp's servers.
pub async fn get_current_changelog(_server_api: Arc<ServerApi>) -> Result<Option<Changelog>> {
    Ok(None)
}
