# Plugin Architecture Overview

## Purpose

This document defines the shared architectural concepts for jm's plugin system. It covers the trait design, plugin lifecycle, data access model, and integration points with the TUI framework.

## Background

The current plugin system supports **sidebar widgets only** — small, 3-6 row panels stacked vertically in a right sidebar (22 columns wide). The existing plugins (Clock, Pomodoro, Notifications) fit this model well, but new plugins like the JIRA integration require full-screen capabilities with their own navigation, modals, and data management.

### Current Plugin Trait (Before Rewrite)

```rust
pub trait Plugin {
    fn name(&self) -> &str;
    fn needs_timer(&self) -> bool { false }
    fn height(&self) -> u16 { 3 }
    fn render(&self, area: Rect, buf: &mut Buffer);
    fn on_tick(&mut self) -> Vec<String> { Vec::new() }
    fn on_key(&mut self, _key: KeyEvent) -> bool { false }
    fn on_notify(&mut self, _message: &str) {}
}
```

**Limitations:**
- No concept of "screen" plugins — everything renders into a fixed sidebar area
- No lifecycle hooks (enter/leave)
- No access to the Action dispatch system (returns `bool`, not `Action`)
- No concept of plugin-owned modals or focus states

## New Architecture

### Trait Design

There is **no base Plugin trait**. `SidebarPlugin` and `ScreenPlugin` are two **independent traits** with no supertrait relationship. Each trait is self-contained with its own `name()` method and lifecycle.

If unified tick/notify dispatch is needed across both kinds, use an `enum AnyPlugin` wrapper:

```
SidebarPlugin  — small widget in the right sidebar (independent trait)
ScreenPlugin   — full-screen view with its own navigation (independent trait)

AnyPlugin      — enum wrapper for unified dispatch (not a trait)
```

```rust
/// Wrapper for unified tick/notify dispatch when needed.
/// This is NOT a trait — it is a concrete enum.
enum AnyPlugin {
    Sidebar(Box<dyn SidebarPlugin>),
    Screen(Box<dyn ScreenPlugin>),
}
```

### SidebarPlugin Trait

For small widgets that render in the sidebar. Backwards-compatible with existing plugins. Renders into a `Buffer` (widget-style).

```rust
pub trait SidebarPlugin {
    /// Unique identifier for this plugin (e.g., "clock", "pomodoro")
    fn name(&self) -> &str;

    /// Whether the plugin needs tick events (called every 1s)
    fn needs_timer(&self) -> bool { false }

    /// Vertical height in rows this plugin needs in the sidebar
    fn height(&self) -> u16 { 3 }

    /// Render the plugin into the given area (widget-style, into Buffer)
    fn render(&self, area: Rect, buf: &mut Buffer);

    /// Called every 1 second if needs_timer() is true.
    /// Returns notification messages to be forwarded to the notification system.
    fn on_tick(&mut self) -> Vec<String> { Vec::new() }

    /// Handle a key event when this plugin has sidebar focus.
    /// Returns true if the key was consumed.
    fn on_key(&mut self, key: KeyEvent) -> bool { false }

    /// Receive a notification message from another plugin or the system.
    fn on_notify(&mut self, _message: &str) {}
}
```

### ScreenPlugin Trait

For full-screen plugins that own their entire view. Renders using `Frame` (full-screen-style). Returns `PluginAction`, not `Action` — the App converts it.

```rust
pub trait ScreenPlugin {
    /// Unique identifier for this plugin (e.g., "jira")
    fn name(&self) -> &str;

    /// Render the full screen into the given area.
    /// Uses Frame (not Buffer) for full-screen rendering.
    fn render(&self, frame: &mut Frame, area: Rect);

    /// Handle a key event. Returns a PluginAction for the app to convert.
    fn handle_key(&mut self, key: KeyEvent) -> PluginAction;

    /// Called when the screen becomes active (screen field set to this plugin).
    /// Use for initial data loading, API calls, thread spawning, etc.
    fn on_enter(&mut self);

    /// Called when the screen is deactivated (navigating away).
    /// Use for cleanup, stopping background threads, etc.
    fn on_leave(&mut self);

    /// Called every 250ms while this plugin's screen is active.
    /// Only called when the plugin screen is the current screen.
    /// Returns notification messages forwarded to PluginSidebar via
    /// push_notification() in the PluginRegistry.
    fn on_tick(&mut self) -> Vec<String> { Vec::new() }

    /// Receive a notification message from another plugin or the system.
    fn on_notify(&mut self, _message: &str) {}

    /// The keybinding hint string shown in the footer bar.
    fn key_hints(&self) -> Vec<(&str, &str)> { Vec::new() }

    /// Called after $EDITOR closes with the edited content.
    /// `context` is the same string passed in PluginAction::LaunchEditor.
    /// Default is a no-op — only override in plugins that use LaunchEditor.
    fn on_editor_complete(&mut self, _content: String, _context: &str) {}
}
```

### PluginAction Enum

Screen plugins return a **narrow** `PluginAction` enum — not the app's full `Action` enum. The App is responsible for converting `PluginAction` into `Action` for processing.

```rust
pub enum PluginAction {
    /// Key was handled, no app-level action needed
    None,
    /// Navigate back to Dashboard
    Back,
    /// Show a toast notification
    Toast(String),
    /// Request the app to launch $EDITOR with the given content.
    /// After the editor closes, the plugin receives the edited content
    /// via `on_editor_complete()`. The `context` string is passed through
    /// so the plugin knows what the edit was for (e.g., "comment:HMI-103").
    LaunchEditor { content: String, context: String },
}
```

The App's conversion is straightforward:

```rust
// In App::handle_key(), when screen is Plugin(name):
match plugin.handle_key(key) {
    PluginAction::None => {},
    PluginAction::Back => {
        plugin.on_leave();
        self.screen = ScreenId::Dashboard;
    }
    PluginAction::Toast(msg) => {
        self.toast = Some(Toast::new(msg));
    }
    PluginAction::LaunchEditor { content, context } => {
        // Write content to a temp file, stash (plugin_name, context, path).
        // The run loop picks this up before the next draw, suspends the TUI,
        // launches $EDITOR, resumes, and calls plugin.on_editor_complete().
        let temp_path = std::env::temp_dir()
            .join(format!("jm-plugin-{}.txt", name));
        std::fs::write(&temp_path, &content).ok();
        self.pending_editor_plugin = Some((name.clone(), context, temp_path));
    }
}
```

## Editor Integration

Screen plugins can request the app to open `$EDITOR` by returning `PluginAction::LaunchEditor` from `handle_key()`. This allows plugins to compose multi-line text (e.g., JIRA comments, issue descriptions) using the user's preferred editor without any screen-plugin-level terminal management.

### How It Works

1. **Plugin returns `PluginAction::LaunchEditor { content, context }`** — `content` is the initial text pre-populated in the editor, `context` is an opaque string the plugin uses to identify what the edit is for (e.g., `"comment:HMI-103"`).
2. **App writes content to a temp file** — `$TMPDIR/jm-plugin-<name>.txt`.
3. **App stashes `(plugin_name, context, temp_path)`** in `pending_editor_plugin`.
4. **At the top of the run loop** (before the next draw), the app detects the pending request, suspends the TUI (`disable_raw_mode`, `LeaveAlternateScreen`), and launches `$EDITOR` (falling back to `vim`).
5. **After the editor exits**, the app resumes the TUI (`enable_raw_mode`, `EnterAlternateScreen`, `terminal.clear()`).
6. **App reads back the temp file content**, deletes the file, and calls `plugin.on_editor_complete(edited_content, context)`.
7. **Plugin processes the result** — e.g., converts to ADF and POSTs as a JIRA comment.

### on_editor_complete Lifecycle Method

```rust
fn on_editor_complete(&mut self, content: String, context: &str) {
    // context tells us what this edit is for
    if let Some(issue_key) = context.strip_prefix("comment:") {
        // Convert plain text to ADF and send as JIRA comment
        self.send_comment(issue_key, content);
    }
}
```

The default implementation is a no-op. Plugins that do not use `LaunchEditor` do not need to implement `on_editor_complete`.

### Screen Plugin Lifecycle with Editor

The lifecycle step "5. Key events" can now result in an editor session:

```
handle_key() -> LaunchEditor { content, context }
    │
    ▼
App writes temp file, sets pending_editor_plugin
    │
    ▼  (next run loop iteration, before draw)
App suspends TUI → launches $EDITOR → resumes TUI
    │
    ▼
plugin.on_editor_complete(edited_content, context)
    │
    ▼
Plugin sends API request / updates state
```

The plugin's screen is **not** deactivated during the editor session. The user returns to the same plugin screen after the editor closes.

## Modal Strategy: Plugin-Owned Modals

Screen plugins manage their own modal state internally. They render overlays in their `render()` call and handle all keys themselves. **The App's modal system is NOT exposed to plugins.**

This means:
- Plugins do NOT return `ShowModal(...)` actions
- Plugins do NOT use `ModalId` from the app
- A screen plugin's `render()` draws its own overlay/popup if it has an active modal
- A screen plugin's `handle_key()` routes keys to its modal when one is open
- The plugin decides internally when to open/close its own modals

Example pattern inside a screen plugin:

```rust
struct JiraPlugin {
    modal: Option<JiraModal>,  // plugin-private modal state
    // ...
}

enum JiraModal {
    IssueDetail(Issue),
    CreateIssue,
    ConfirmTransition(Issue, String),
}

impl ScreenPlugin for JiraPlugin {
    fn render(&self, frame: &mut Frame, area: Rect) {
        // Render main board view
        self.render_board(frame, area);

        // If a modal is active, render it as an overlay
        if let Some(modal) = &self.modal {
            self.render_modal(frame, area, modal);
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> PluginAction {
        // Modal gets first crack at keys
        if let Some(modal) = &mut self.modal {
            return self.handle_modal_key(key, modal);
        }
        // Otherwise handle board-level keys
        self.handle_board_key(key)
    }
}
```

## Data Access Model

### Self-Contained Plugins

Screen plugins are **self-contained** — they manage their own data and do not have access to the core jm stores (ProjectStore, IssueStore, JournalStore, etc.).

**Rationale:**
- Clean separation of concerns
- Plugins cannot corrupt local jm data
- External integrations (JIRA, GitHub, etc.) have their own data sources
- No coupling between plugin data models and core models

**Implications:**
- Screen plugins fetch and cache their own data (e.g., from APIs)
- No cross-referencing between plugin data and local jm data
- Plugins communicate with the app only through the `PluginAction` enum

### Communication via PluginAction

Screen plugins return `PluginAction` values from `handle_key()`. The app's event loop converts these into internal `Action` values for processing. The `PluginAction` enum is intentionally narrow — `None`, `Back`, `Toast(String)`, and `LaunchEditor { content, context }`.

## Plugin Registration & Configuration

### Configuration Schema

Plugins are configured in `~/.jm/config.yaml`:

```yaml
plugins:
  enabled: [pomodoro, notifications, clock, jira]

  # Sidebar plugin configs
  pomodoro:
    work_minutes: 25
  notifications:
    reminders:
      - time: "09:00"
        message: "Morning review"

  # Screen plugin configs
  jira:
    url: "https://myorg.atlassian.net"
    email: "matt@company.com"
    refresh_interval_secs: 60
```

### Config Extension via serde(flatten)

To support arbitrary plugin-specific config without modifying jm-core for each new plugin, the `PluginConfig` struct uses `serde(flatten)`:

```rust
#[derive(Deserialize)]
pub struct PluginConfig {
    pub enabled: Vec<String>,

    // Known plugin configs (optional)
    pub pomodoro: Option<PomodoroConfig>,
    pub notifications: Option<NotificationsConfig>,

    // Everything else — plugins deserialize from raw Value
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yml::Value>,
}
```

Plugins deserialize their own config from the raw `Value`:

```rust
// During registration, screen plugins extract their config:
if let Some(raw) = config.extra.get("jira") {
    let jira_config: JiraConfig = serde_yml::from_value(raw.clone())?;
    screen_plugins.push(Box::new(JiraPlugin::new(jira_config)));
}
```

This keeps jm-core unchanged when adding new plugins — new plugin config sections are captured by `extra` and deserialized by the plugin itself.

### Registration

Plugins are registered statically in the `PluginRegistry::new()` factory. The factory reads the `enabled` list and instantiates the appropriate plugin type:

```rust
// Pseudocode for plugin registration
match plugin_name {
    "clock" => sidebar_plugins.push(Box::new(ClockPlugin::new())),
    "pomodoro" => sidebar_plugins.push(Box::new(PomodoroPlugin::new(config))),
    "notifications" => sidebar_plugins.push(Box::new(NotificationsPlugin::new(config))),
    "jira" => screen_plugins.push(Box::new(JiraPlugin::new(config))),
    _ => {} // silently skip unknown plugins
}
```

### Plugin Registry

The app holds both types. Screen plugins are stored in a `Vec`, not a `HashMap`:

```rust
pub struct PluginRegistry {
    pub sidebar: PluginSidebar,                   // existing sidebar container
    pub screens: Vec<Box<dyn ScreenPlugin>>,      // screen plugins (Vec, not HashMap)
}
```

Screen plugins are looked up by iterating and matching on `name()`. The list is small (typically 1-3 screen plugins), so linear scan is fine.

## Notification Forwarding

When a screen plugin's `on_tick()` returns notification messages, those messages must be forwarded to the sidebar's notification plugin. The `PluginRegistry` handles this:

```rust
impl PluginRegistry {
    pub fn tick_active_screen(&mut self, active_screen: &ScreenId) {
        if let ScreenId::Plugin(name) = active_screen {
            if let Some(plugin) = self.screens.iter_mut().find(|p| p.name() == name) {
                let notifications = plugin.on_tick();
                for msg in notifications {
                    self.sidebar.push_notification(&msg);
                }
            }
        }
    }
}
```

This ensures screen plugin notifications appear in the sidebar's notification center even though the sidebar is hidden while the screen plugin is active. Notifications are queued and visible when the user returns to the dashboard.

## Lifecycle

### Sidebar Plugin Lifecycle

1. **Instantiation** — `new(config)` in `PluginRegistry::new()`
2. **Render loop** — `render()` called every frame when sidebar is visible
3. **Tick** — `on_tick()` called every 1s if `needs_timer()` is true
4. **Key events** — `on_key()` called when sidebar is focused at this plugin's index
5. **Notifications** — `on_notify()` called when another plugin emits a message

### Screen Plugin Lifecycle

1. **Instantiation** — `new(config)` during app initialization
2. **Enter** — `on_enter()` called when user navigates to the plugin screen
3. **Render loop** — `render()` called every frame while the screen is active; plugin gets full terminal width (sidebar is hidden)
4. **Tick** — `on_tick()` called every **250ms** while this plugin's screen is active. **Not called when the plugin is inactive.** Notification messages returned from `on_tick()` are forwarded to `PluginSidebar` via `push_notification()` in the `PluginRegistry`.
5. **Key events** — `handle_key()` called for every key event while screen is active; returns `PluginAction`
6. **Leave** — `on_leave()` called when user navigates away (Back action returns to Dashboard)
7. **Notifications** — `on_notify()` for inter-plugin messaging

### Tick Rate Summary

| Plugin type   | Tick rate | When ticked               |
|---------------|-----------|---------------------------|
| SidebarPlugin | 1s        | Always (if needs_timer()) |
| ScreenPlugin  | 250ms     | Only when screen is active |

## Focus & Navigation

### Screen Field (Flat Navigation)

Screen plugins integrate with the existing `ScreenId` system. Navigation uses a **flat screen field** — NOT a stack. There is no push/pop. `self.screen` is set directly, and Back always returns to Dashboard.

```rust
pub enum ScreenId {
    Dashboard,
    ProjectView(String),
    Switch(Option<String>),
    Review,
    Search,
    People,
    IssueBoard,
    Weekly,
    Plugin(String),     // Dynamic screen ID for plugins
}
```

When a screen plugin is opened:
1. `self.screen = ScreenId::Plugin("jira".into())` — flat assignment, not push
2. `plugin.on_enter()` is called
3. Sidebar is hidden — plugin gets full terminal width
4. All key events route to `plugin.handle_key()`
5. Rendering delegates to `plugin.render()`
6. When the plugin returns `PluginAction::Back`, the screen field is set to `ScreenId::Dashboard` and `plugin.on_leave()` is called

Back **always** returns to Dashboard. There is no nested screen history.

### Sidebar Visibility

The sidebar is **hidden** when a screen plugin is active. The screen plugin gets the full terminal width for rendering. When the user navigates back to Dashboard (or any non-plugin screen), the sidebar reappears.

### Entry Keybinding

Screen plugins are opened with uppercase letter keybindings, consistent with other screen-level shortcuts:

| Key | Screen          |
|-----|-----------------|
| `I` | Issue Board     |
| `W` | Weekly Review   |
| `J` | JIRA (plugin)   |

The keybinding is `J` (uppercase), NOT `Ctrl+J`. This is consistent with the existing pattern where uppercase letters open full-screen views.

### Plugin Keybinding Discovery

Plugin keybindings are handled in `App::handle_key()` as a **fallback layer**, after the current screen's handler returns None/unhandled:

```rust
// In App::handle_key():
fn handle_key(&mut self, key: KeyEvent) -> Action {
    // 1. Current screen gets first crack
    let action = match &self.screen {
        ScreenId::Dashboard => self.handle_dashboard_key(key),
        ScreenId::Plugin(name) => { /* route to plugin */ },
        // ... other screens ...
    };

    // 2. If unhandled, check plugin keybindings (fallback layer)
    if action == Action::None {
        match key {
            // Hardcoded initially — no dynamic registration needed
            KeyEvent { code: KeyCode::Char('J'), .. } => {
                return self.open_plugin_screen("jira");
            }
            _ => {}
        }
    }

    action
}
```

Keybindings are **hardcoded initially**. Dynamic keybinding registration from config can be added later if needed. This avoids premature abstraction.

## Background Work Pattern

For plugins that need to perform async operations (API calls, file I/O) without blocking the TUI:

```
┌─────────┐     channel      ┌──────────────┐
│ TUI     │ <──────────────── │ Background   │
│ Thread  │                   │ Thread       │
│         │ ──────────────► │              │
│ render  │    command        │ API calls    │
│ keys    │                   │ processing   │
└─────────┘                   └──────────────┘
```

### Thread Lifecycle

1. **Spawn on `on_enter()`** — plugin spawns a background thread. Uses a **thread respawn guard**: if a thread is already running (e.g., rapid enter/leave/enter), skip spawning and reuse the existing one.
2. **Commands sent** via `mpsc::Sender` (fetch, update, create)
3. **Results received** via `mpsc::Receiver`, drained in `on_tick()` using a **channel drain loop**: `while let Ok(result) = receiver.try_recv() { /* process */ }`
4. **Shutdown signal** — `AtomicBool` shared between TUI and background thread. Set to `true` on `on_leave()`. The background thread checks this flag between operations and exits cleanly.
5. **Thread stopped on `on_leave()`** — shutdown signal set, thread exits on next check.

### HTTP Client

Each background thread creates a **single `ureq::Agent`** (HTTP client) for its lifetime. The client is NOT shared across thread respawns — each thread owns its own.

```rust
// Inside background thread
fn background_main(
    shutdown: Arc<AtomicBool>,
    commands: mpsc::Receiver<Command>,
    results: mpsc::Sender<ApiResult>,
) {
    // Single client for the lifetime of this thread
    let client = ureq::Agent::new();

    while !shutdown.load(Ordering::Relaxed) {
        match commands.recv_timeout(Duration::from_millis(100)) {
            Ok(cmd) => {
                let result = execute_command(&client, cmd);
                let _ = results.send(result);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}
```

### Result Processing in on_tick()

```rust
fn on_tick(&mut self) -> Vec<String> {
    let mut notifications = Vec::new();

    // Drain all available results — do not stop at first
    while let Ok(result) = self.result_rx.try_recv() {
        match result {
            ApiResult::BoardData(data) => {
                self.board = data;
                self.loading = false;
            }
            ApiResult::Error(msg) => {
                notifications.push(format!("JIRA error: {}", msg));
                self.loading = false;
            }
            // ...
        }
    }

    notifications
}
```

This pattern keeps the TUI responsive during network operations.

## File Layout

After the rewrite, the plugin directory structure:

```
crates/jm-tui/src/plugins/
├── mod.rs              # SidebarPlugin + ScreenPlugin traits (independent, no base trait)
│                       #   + PluginAction enum { None, Back, Toast(String), LaunchEditor { content, context } }
│                       #   + AnyPlugin enum wrapper (if needed for unified dispatch)
├── registry.rs         # PluginRegistry { sidebar: PluginSidebar, screens: Vec<Box<dyn ScreenPlugin>> }
│                       #   + tick_active_screen() with notification forwarding
├── sidebar.rs          # PluginSidebar container (sidebar rendering/focus)
├── clock.rs            # Clock sidebar plugin (impl SidebarPlugin)
├── notifications.rs    # Notifications sidebar plugin (impl SidebarPlugin)
├── pomodoro.rs         # Pomodoro sidebar plugin (impl SidebarPlugin)
└── jira/               # JIRA screen plugin (module directory)
    ├── mod.rs           # JiraPlugin struct implementing ScreenPlugin
    │                    #   render(&self, frame: &mut Frame, area: Rect)
    │                    #   handle_key() -> PluginAction
    │                    #   Plugin-owned modal state (JiraModal enum)
    ├── api.rs           # JIRA Cloud REST v3 client (ureq)
    ├── models.rs        # JIRA data types (Issue, Project, Status, etc.)
    ├── board.rs         # Kanban board rendering
    ├── detail.rs        # Issue detail modal rendering (plugin-owned, not App modal)
    └── config.rs        # JIRA-specific configuration (deserialized from serde_yml::Value)
```

### Trait Signature Summary

```rust
// In mod.rs — two independent traits, no supertrait

pub trait SidebarPlugin {
    fn name(&self) -> &str;
    fn needs_timer(&self) -> bool { false }
    fn height(&self) -> u16 { 3 }
    fn render(&self, area: Rect, buf: &mut Buffer);      // Buffer (widget-style)
    fn on_tick(&mut self) -> Vec<String> { Vec::new() }   // 1s tick rate
    fn on_key(&mut self, key: KeyEvent) -> bool { false }
    fn on_notify(&mut self, _message: &str) {}
}

pub trait ScreenPlugin {
    fn name(&self) -> &str;
    fn render(&self, frame: &mut Frame, area: Rect);      // Frame (full-screen-style)
    fn handle_key(&mut self, key: KeyEvent) -> PluginAction;
    fn on_enter(&mut self);
    fn on_leave(&mut self);
    fn on_tick(&mut self) -> Vec<String> { Vec::new() }   // 250ms tick rate, active only
    fn on_notify(&mut self, _message: &str) {}
    fn key_hints(&self) -> Vec<(&str, &str)> { Vec::new() }
    fn on_editor_complete(&mut self, _content: String, _context: &str) {} // default no-op
}

pub enum PluginAction {
    None,
    Back,
    Toast(String),
    LaunchEditor { content: String, context: String },
}
```
