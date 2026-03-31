# Concurrency Review v2 — JIRA Plugin Threading Model

**Reviewer**: Concurrency engineer, background thread patterns in Rust TUI apps
**Date**: 2026-03-27
**Question asked**: Will the threading and async data flow work correctly without races, deadlocks, or data corruption?

---

## Files Reviewed

- `docs/design/jira-plugin.md` — threading model, refresh behavior, optimistic UI
- `docs/design/plugin-architecture.md` — background thread pattern
- `docs/design/jira-api-reference.md` — pagination, rate limiting
- `crates/jm-tui/src/plugins/mod.rs` — ScreenPlugin trait
- `crates/jm-tui/src/app.rs` — tick timers, event loop, editor suspend
- `crates/jm-tui/src/plugins/registry.rs` — tick_screen method

---

## Area-by-Area Verdict

---

### 1. Channel Pattern

**PASS**

The spec describes two `mpsc` channels: a command channel (TUI sends, background thread receives) and a result channel (background thread sends, TUI receives). Ownership is correctly assigned: `command_tx: Option<mpsc::Sender<JiraCommand>>` and `result_rx: Option<mpsc::Receiver<JiraResult>>` live in `JiraPlugin` (TUI side); the background thread owns the `Receiver<JiraCommand>` and `Sender<JiraResult>`. This is the standard, sound Rust `mpsc` pattern — no shared mutable state crosses the thread boundary, and the `mpsc` types enforce the correct ownership at compile time.

The use of `Option<...>` for all four handles is correct: they are `None` before `on_enter()` fires and after `on_leave()` tears down the thread, preventing use-after-cleanup.

No concern.

---

### 2. Thread Lifecycle and Rapid Open/Close

**CONCERN**

The spec says `on_enter()` checks `JoinHandle::is_finished()` before spawning a new thread. This is the thread respawn guard. The intent is sound, but the interaction between the guard and the shutdown signal has a window:

**Scenario**: User opens JIRA screen → `on_enter()` spawns thread T1 → user immediately presses Esc → `on_leave()` sets `shutdown_flag = true` and sends `Shutdown` command → user immediately presses J again → `on_enter()` fires.

At this point `is_finished()` may still return `false` because T1 has not had a chance to observe the shutdown flag and exit. The guard skips spawning a new thread and reuses T1's channels. But T1 is still draining its current command and is about to exit as soon as it sees the flag. The result: the "reused" thread is one loop iteration from exiting, and then both the command_tx and result_rx on the TUI side become disconnected senders/receivers pointing at a dead thread.

**Impact**: On the next `try_recv()` call, the TUI will get `TryRecvError::Disconnected`, triggering the "reconnect prompt" error path — which is spec'd as the panic-detection path, not the normal lifecycle path. The user will see an unexpected error modal after rapid open/close/open.

**Fix needed in spec**: After the guard check, if the thread is still running but the shutdown flag is set, the spec should say: reset the shutdown flag (create a new `Arc<AtomicBool>`) and send a `FetchMyIssues` to restart normal operation. Alternatively, use a counter in the shutdown flag (generation-based shutdown) so a fresh on_enter can "cancel" the pending shutdown and reuse the thread without tearing down the channels.

This is a recoverable annoyance rather than data corruption, but it will manifest as a confusing UX on rapid tab switching.

---

### 3. Channel Drain in on_tick

**PASS with observation**

`while let Ok(result) = try_recv()` drains all pending results per 250ms tick. This is correct — it is the only sound way to avoid stale results accumulating across ticks. Stopping at the first result would leave a growing backlog.

**Observation on backlog**: The channel can back up in one scenario: a multi-page paginated fetch (e.g., 500 issues across 5 pages of 100) completes all pages before on_tick fires, and sends the `Issues` result as a single message. The spec says "sends the full `Vec<JiraIssue>` once complete" — meaning the background thread accumulates all pages into one Vec and sends a single `Issues` result. This means channel backlog is bounded to roughly one message per in-flight command type, not one per API page. That is the correct design. No overflow concern.

250ms is fast enough for any realistic JIRA response time (even slow cloud JIRA instances respond within seconds). The `recv_timeout(100ms)` in the background thread ensures it checks the shutdown flag frequently. No concern.

---

### 4. Generation Counter

**CONCERN**

The generation counter is correct for its stated purpose: preventing a slow earlier fetch from overwriting a newer fetch. But the spec has a gap in how `refreshing` and `generation` interact.

**Scenario** (write-then-refresh race):

1. User triggers transition → write command sent → `refreshing = true`, `generation` incremented to G1, `FetchMyIssues { generation: G1 }` queued with 500ms delay
2. Auto-refresh timer fires at ~60s boundary → `refreshing` is already `true` → auto-refresh is skipped (correct, deduplication works)
3. The 500ms-delayed fetch fires → sends `FetchMyIssues { generation: G1 }`
4. While T1's pages are still being fetched, user presses `R` → `generation` incremented to G2, `FetchMyIssues { generation: G2 }` sent
5. T1 completes and sends `Issues { generation: G1 }` → TUI discards it (correct)
6. T2 completes and sends `Issues { generation: G2 }` → TUI applies it (correct)

The generation counter handles this correctly. The issue is more subtle: the spec does not specify what resets `refreshing`. If `refreshing` is only cleared when an `Issues` result arrives, and the `Issues` result for G1 is discarded (because G2 is current), then `refreshing` will never be cleared from the G1 fetch. The auto-refresh timer will be blocked permanently until G2 completes.

**Specifically**: if the sequence is G1 sets `refreshing = true`, G2 is sent before G1 result arrives, G1 result arrives and is discarded (generation mismatch) — `refreshing` may never be cleared if the clearing logic is tied to `generation == current_generation`. The spec text says "A `refreshing: bool` flag prevents overlapping refreshes" but does not specify the exact clearing condition.

**Recommendation**: Clear `refreshing` on any `Issues` result arrival regardless of generation. The generation counter already handles the stale-data protection; `refreshing` only needs to gate whether to send a new `FetchMyIssues`.

---

### 5. Optimistic UI and Refresh Race

**CONCERN**

The spec says: after sending a transition command, optimistically move the issue to the target column locally. If `TransitionFailed` arrives, revert to the original column.

The race condition: what if a `FetchMyIssues` result arrives between the optimistic move and `TransitionComplete`/`TransitionFailed`?

**Scenario**:
1. Issue HMI-103 is in "In Progress". User transitions to "Code Review".
2. Plugin applies optimistic move: `issues` updated to show HMI-103 in "Code Review".
3. Auto-refresh fires (independently scheduled, not suppressed by `refreshing` if transition did not set it). Background thread sends back stale `Issues` result that still shows HMI-103 in "In Progress".
4. TUI applies the refresh result → HMI-103 snaps back to "In Progress" visually.
5. `TransitionComplete` arrives → plugin does nothing (transition succeeded) but board already shows stale state.

**Severity**: If `refreshing` is set to `true` when the transition is sent (before the 500ms delayed post-write refresh), and `refreshing` suppresses auto-refresh, this race is prevented. But the spec is not explicit that an in-flight transition sets `refreshing`. The spec says the 500ms delay triggers a refresh *after* the write, and `refreshing` prevents *duplicate refreshes* — but it is not clear that a transition command sets `refreshing` immediately.

If `refreshing` is only set when `FetchMyIssues` is sent (not when `TransitionIssue` is sent), there is a window during the transition latency where an auto-refresh can arrive and clobber the optimistic state.

**Fix needed in spec**: When any write command is sent (`TransitionIssue`, `UpdateField`, `AddComment`, `CreateIssue`), set `refreshing = true` immediately to suppress auto-refresh. Clear `refreshing` when the write result arrives (not after the post-write refresh completes). The 500ms-delayed `FetchMyIssues` then fires with `refreshing` still available to be re-set.

---

### 6. Write-Then-Refresh and refreshing Flag

**CONCERN** (related to Area 5)

The spec says: "500ms delay after write before refresh. `refreshing` flag prevents duplicate refreshes."

The 500ms delay is implemented where? The spec describes the behavior but not the mechanism. There are two possible implementations:

**Option A**: Background thread receives `TransitionIssue`, executes the API call, then `std::thread::sleep(500ms)`, then sends `FetchMyIssues` to itself (impossible — the command channel is one-directional from TUI to thread).

**Option B**: `TransitionComplete` arrives on the TUI side → TUI records `post_write_refresh_at = Instant::now() + 500ms`. The 250ms `on_tick` checks if `post_write_refresh_at` has elapsed and fires the refresh.

Option B is the only architecturally sound choice given the channel design, but the spec does not specify this. If implementation uses `std::thread::sleep(500ms)` inside the background thread before sending `FetchMyIssues` to the command channel (which is impossible since the thread reads from the command channel, not writes to it), the implementation would be broken.

**The spec needs to explicitly say**: "On `TransitionComplete` (and other write results), the TUI records a timestamp and fires `FetchMyIssues` after 500ms via the normal `on_tick` timer mechanism."

Additionally, there is a secondary question: does the `refreshing` flag get set when the 500ms-delayed refresh fires, or when `TransitionComplete` arrives? If set on `TransitionComplete`, the auto-refresh is suppressed for 500ms, which is correct behavior. If not set until the delayed `FetchMyIssues` is actually sent, there is a 500ms window where an auto-refresh could fire and clobber the optimistic state. See Area 5.

---

### 7. $EDITOR Suspend and Background Thread

**PASS**

The app.rs code is clear: the editor suspend blocks the TUI thread (`std::process::Command::new(&editor).arg(&temp_path).status()` — a blocking wait). The background thread is unaffected; it continues running, receiving results into the channel. On editor close, the run loop resumes and `on_tick()` is called normally, which drains accumulated results from the channel.

This is correct behavior. The result channel is a bounded-capacity `mpsc` channel (unbounded by default in `std::sync::mpsc`), so results accumulate without blocking or dropping. No data is lost during the editor session.

The channel might accumulate one or two auto-refresh results if the user spends time in the editor. These are harmless — the drain loop processes all of them in the next `on_tick()`.

The one scenario worth noting: the user is in the editor for >60 seconds. Two `Issues` results may arrive. The generation counter ensures only the most recent one is applied. No concern.

---

### 8. Panic in Background Thread / Disconnected Detection

**PASS with observation**

The spec says: if `try_recv()` returns `TryRecvError::Disconnected`, the background thread has panicked — show a reconnect prompt.

This is correct behavior: `mpsc::Sender` is dropped when the thread exits (for any reason including panic), causing subsequent `try_recv()` calls to return `Disconnected`.

**Observation**: The spec conflates "panic" with "disconnected." In the normal `on_leave()` path, the channels are also dropped (the background thread exits cleanly after seeing the shutdown flag). So `Disconnected` will also fire in the normal teardown path if `on_tick()` is called after `on_leave()`. The spec should clarify: `Disconnected` is only an error if the plugin screen is still active (i.e., `on_leave()` has not been called). The registry's `tick_screen` is only called when the screen is active (`ScreenId::Plugin(name)`), so in practice this would only occur if the thread panicked unexpectedly. But the spec should document this assumption explicitly.

The "reconnect prompt" error recovery path is reasonable — it gives the user a way to manually trigger `on_enter()` which will spawn a fresh background thread.

---

### 9. ureq Blocking in std::thread

**PASS**

`ureq` is a synchronous HTTP client. Running blocking I/O in a `std::thread` (not in an async executor) is the correct, idiomatic pattern. There is no tokio thread pool to starve. The TUI main thread is never blocked — it uses `event::poll(100ms)` and the plugin result channel is non-blocking (`try_recv`).

A single `ureq::Agent` per thread lifetime reuses connection pools, which is the correct usage. Creating a new agent per request would be wasteful (no connection reuse) and creating a shared agent across thread respawns would require `Arc<Mutex<Agent>>` and risk blocking.

The `recv_timeout(100ms)` in the background thread ensures it checks the shutdown flag 10 times per second even when idle. No deadlock risk.

The spec references `ureq::Agent::new()` or `ureq::AgentBuilder::new().build()` for v3, distinguishing from the v2 `ureq::agent()` call. This is a correct and important distinction. No concern.

---

## Summary Table

| Area | Verdict | Notes |
|------|---------|-------|
| 1. Channel pattern | PASS | Correct mpsc ownership, Options guard pre/post lifecycle |
| 2. Thread lifecycle / rapid open-close | CONCERN | Respawn guard window: thread may be exiting when on_enter reuses channels; user sees Disconnected error |
| 3. Channel drain (on_tick) | PASS | Drain loop correct; single-message-per-fetch design prevents backlog |
| 4. Generation counter | CONCERN | refreshing may never clear if generation mismatch causes Issues result to be discarded |
| 5. Optimistic UI / refresh race | CONCERN | Auto-refresh can clobber optimistic state during transition latency if refreshing not set on write command send |
| 6. Write-then-refresh (500ms delay) | CONCERN | Implementation mechanism for 500ms delay is unspecified; refreshing window may not cover full transition latency |
| 7. $EDITOR suspend | PASS | Background thread unaffected; accumulated results drained on resume; channel is unbounded |
| 8. Panic / Disconnected detection | PASS with observation | Conflates panic with normal teardown Disconnected; safe in practice but needs doc clarification |
| 9. ureq blocking in std::thread | PASS | Correct pattern; no tokio starvation; single Agent per thread is idiomatic |

---

## Critical Issues Requiring Spec Fixes Before Implementation

### C1 — refreshing never clears on generation mismatch (Area 4)

**Scenario**: G1 FetchMyIssues is in-flight. User sends R → G2 FetchMyIssues. G1 result arrives, is discarded (generation != current). If `refreshing` is cleared only on generation-match, it stays `true` permanently. Auto-refresh is blocked.

**Required fix**: Clear `refreshing` on any `Issues` result, regardless of generation.

---

### C2 — Optimistic state clobbered by auto-refresh during write latency (Area 5 / 6)

**Scenario**: Transition sent. Optimistic move applied. Before `refreshing` is set (or before the post-write FetchMyIssues is sent), auto-refresh timer fires and sends back pre-transition state. Board snaps back. TransitionComplete arrives but board shows stale state.

**Required fix**: Set `refreshing = true` when any write command is sent (not just when FetchMyIssues is sent). Clear `refreshing` when the write result (TransitionComplete, FieldUpdated, etc.) arrives.

---

### C3 — 500ms delay mechanism is unspecified (Area 6)

The background thread cannot send commands to itself. The only correct implementation is a TUI-side timer. The spec must specify: "On write result arrival, record `post_write_refresh_deadline = Instant::now() + 500ms`. In `on_tick()`, if deadline is past and `refreshing` is false, send `FetchMyIssues`."

---

### C4 — Rapid open/close may produce spurious Disconnected error (Area 2)

The respawn guard (`is_finished()`) does not distinguish "thread running normally" from "thread is shutting down due to on_leave()". On rapid open/close/open, the guard may reuse channels of a thread that is one iteration from exiting.

**Required fix**: The spec should say: when `on_enter()` finds `is_finished() == false` but `shutdown_flag == true` (thread is in the process of shutting down), wait for the thread to finish (`join()` with a short timeout or spin on `is_finished()`) before spawning a fresh thread with fresh channels. Alternatively, use a generation counter in the shutdown flag so a new on_enter can atomically cancel the pending shutdown.

---

## Verdict

**REJECT**

The channel topology and single-thread-per-plugin pattern are sound. The ureq choice, channel drain loop, and editor-suspend handling are all correct. However, four specification gaps — the `refreshing` flag never clearing on generation mismatch (C1), the optimistic state / auto-refresh race during write latency (C2), the unspecified 500ms delay mechanism (C3), and the rapid open/close Disconnected error (C4) — would produce observable bugs in the implemented code. C2 in particular is a data-integrity issue (UI shows wrong state after a successful transition). These must be resolved in the spec before implementation begins.

None of the issues are architectural dead-ends. All four have straightforward fixes that do not require changing the channel topology or threading model. Fix the four items above and resubmit.
