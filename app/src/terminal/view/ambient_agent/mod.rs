mod block;
mod first_time_setup;
mod footer;
mod harness_selector;
mod host_selector;
mod loading_screen;
mod model;
mod model_selector;
mod progress;
mod progress_ui_state;
mod tips;
mod view_impl;

pub use block::*;
pub use first_time_setup::{FirstTimeCloudAgentSetupView, FirstTimeCloudAgentSetupViewEvent};
pub use footer::{render_error_footer, render_loading_footer};
pub use harness_selector::{HarnessSelector, HarnessSelectorAction, HarnessSelectorEvent};
pub use host_selector::{
    Host, HostSelector, HostSelectorAction, HostSelectorEvent, NakedHeaderButtonTheme,
};
pub use loading_screen::{render_cloud_mode_error_screen, render_cloud_mode_loading_screen};
pub use model::{AgentProgress, AmbientAgentViewModel, AmbientAgentViewModelEvent, Status};
pub use model_selector::{ModelSelector, ModelSelectorAction, ModelSelectorEvent};
pub use progress::{render_progress, ProgressProps, ProgressStep, ProgressStepState};
pub use progress_ui_state::AmbientAgentProgressUIState;
pub use tips::{get_cloud_mode_tips, CloudModeTip};
use crate::ai::blocklist::agent_view::AgentViewController;
use crate::pane_group::TerminalViewResources;
use crate::terminal::shared_session;
use crate::terminal::TerminalManager;
use crate::terminal::TerminalView;
use warpui::geometry::vector::Vector2F;
use warpui::{AppContext, ModelHandle, SingletonEntity, ViewHandle, WindowId};

/// Creates a cloud mode terminal view and manager for ambient agent sessions.
///
/// This is used when pushing a new ambient agent view onto an existing pane's navigation stack,
/// or when creating a standalone ambient agent pane.
pub fn create_cloud_mode_view(
    resources: TerminalViewResources,
    view_bounds_size: Vector2F,
    window_id: WindowId,
    ctx: &mut AppContext,
) -> (
    ViewHandle<TerminalView>,
    ModelHandle<Box<dyn TerminalManager>>,
) {
    // In Cloud Mode, ambient agent prompts are composed in an uninitialized session-sharing
    // viewer pane. This lets us reuse the terminal input without a backing session, and
    // then join the ambient agent session once it's ready.
    let terminal_manager: ModelHandle<Box<dyn TerminalManager>> = ctx.add_model(|ctx| {
        Box::new(shared_session::viewer::TerminalManager::new_deferred(
            resources,
            view_bounds_size,
            window_id,
            ctx,
        )) as Box<dyn TerminalManager>
    });

    let terminal_view = terminal_manager.as_ref(ctx).view();

    // Subscribe to the ambient agent view model to join the session once it's ready.
    // This ensures that we use the manager corresponding to this specific view.
    let view_model = terminal_view.as_ref(ctx).ambient_agent_view_model().clone();
    terminal_manager.update(ctx, |_, ctx| {
        ctx.subscribe_to_model(&view_model, move |manager, event, ctx| {
            if let AmbientAgentViewModelEvent::SessionReady { session_id } = event {
                if let Some(manager) = manager
                    .as_any_mut()
                    .downcast_mut::<shared_session::viewer::TerminalManager>()
                {
                    manager.connect_to_session(*session_id, ctx);
                }
            };
        });
    });

    (terminal_view, terminal_manager)
}

/// Returns `true` when a cloud agent shared session is ready but no agent exchange has been
/// received yet. In this state, we hide the interactive input and render a loading footer
/// instead.
pub fn is_cloud_agent_pre_first_exchange(
    _ambient_agent_view_model: &ModelHandle<AmbientAgentViewModel>,
    _agent_view_controller: &ModelHandle<AgentViewController>,
    _app: &AppContext,
) -> bool {
    // CloudMode is off in slim → no cloud agents exist, so this guard
    // is always false. Kept as a hardcoded false (rather than collapsed
    // at every external caller) so call sites in status_bar /
    // shared_session viewer / setup_command_text don't need a
    // coordinated rewrite.
    false
}
