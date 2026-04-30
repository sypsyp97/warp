use warpui::{Entity, ModelContext, SingletonEntity};

pub struct UsageHistoryModel {
    entries: Vec<warp_graphql::queries::get_conversation_usage::ConversationUsage>,
    is_loading: bool,
    // Whether the server indicated that there may be more entries to load.
    has_more_entries: bool,
}

impl Entity for UsageHistoryModel {
    type Event = ();
}

impl SingletonEntity for UsageHistoryModel {}

impl UsageHistoryModel {
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            entries: Vec::new(),
            is_loading: false,
            has_more_entries: false,
        }
    }

    pub fn entries(&self) -> &[warp_graphql::queries::get_conversation_usage::ConversationUsage] {
        &self.entries
    }

    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    pub fn has_more_entries(&self) -> bool {
        self.has_more_entries
    }

    /// Slim fork: no remote usage history; both refresh and load-more
    /// are no-ops.
    pub fn refresh_usage_history_async(&mut self, _ctx: &mut ModelContext<Self>) {}

    pub fn load_more_usage_history_async(&mut self, _ctx: &mut ModelContext<Self>) {}
}
