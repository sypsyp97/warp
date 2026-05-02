---
status: done
updated: 2026-05-02T03:30:00Z
---

# Goal
Slim the warp fork down to the irreducible core — delete every cloud/billing/team subsystem that is dead-by-design in slim, until no further safe deletion remains.

# Done when
- [x] AWS Bedrock subsystem ripped out — `80fd981`, –2682 LOC, 8 files deleted, 40 modified
- [x] Three dead modals deleted — `a9e01ab`, –2288 LOC, 3 files deleted, 16 modified
- [x] PricingInfoModel + pricing/mod.rs deleted — `ca935a2`, –139 LOC
- [x] Telemetry events.rs trimmed — `e834da3`, –648 LOC, 54 of 441 variants removed
- [x] Final sweep: every remaining `// Slim fork:` marker that points to genuinely dead code is either deleted or annotated as kept-for-exhaustiveness
- [x] All commits pushed; cargo build + clippy + test compilation all clean

# Constraints
- Working dir: C:\Users\sun\Downloads\warp-fork, branch `slim`
- One major deletion per commit; commit message follows existing `slim: ...` style with Co-Authored-By trailer
- Build must stay clean after every commit (cargo check + cargo clippy -D warnings + cargo check --tests)
- Push after each commit (or batched 2-3 if rapid)
- Don't touch drive, cloud_object, or team-management cascades — the summary classified those as too-coupled
- Don't delete `send_telemetry_from_ctx!` macro or telemetry infrastructure; only trim the events enum
- Use Agent delegation for cascades that span >5 files; do surgical edits in-line for single-file work
- Never use `--no-verify` or skip hooks; never force-push

# Final-sweep commits
- `428c123` — drive index Create/Join Team UI (–411 LOC)
- `b16bc93` — collapse no-op upgrade/billing/team-settings event chains (–164 LOC)
- `6c3e694` — drop dead OpenBillingAndUsagePane action + unreachable upsell (–48 LOC)
- `fdc5c41` — drop unreachable personal-object-limit status card (–46 LOC)

Final-sweep total: –669 LOC across 4 commits.

# Markers kept (intentional)
~38 `// Slim fork:` markers remain. They split into three categories, all
load-bearing:

1. Doc comments on slim stubs (auth_state, auth_manager, server_api,
   server_api/auth, telemetry/mod, telemetry/collector, settings/privacy,
   ai/agent_sdk/admin, ai/agent_sdk/driver, ai/execution_profiles/editor,
   ai/mcp/templatable_manager/oauth, ai/predict/next_command_model,
   ai/blocklist/passive_suggestions/legacy, crates/ai/api_keys,
   crates/command-signatures-v2/build.rs).
2. Kept-for-exhaustiveness match arms whose event variants live in
   external crates we don't own (terminal/view/ambient_agent/{view_impl,model},
   root_view AgentOnboardingEvent stubs, workspace/view Reauth handler,
   uri/mod team-host fallthrough, settings_view/mod nav-items doc,
   workspace/view 7881 menu builder).
3. New explanatory comments left by this sweep on collapsed dead branches
   (shared_objects_creation_denied_modal, terminal/shared_session/share_modal,
   prompt_alert).

No further safe deletion remains in this sweep without crossing into
drive/cloud_object/team-management cascades, which were classified
too-coupled at the start.
