# Phase 0: Plugin System Rewrite

## Objective

Rewrite the jm plugin system to support both **sidebar widgets** and **full-screen plugins**. This is a prerequisite for the JIRA plugin (Phase 1) and any future screen-based plugins.

## Scope

### In Scope

1. Define two independent traits: `SidebarPlugin` + `ScreenPlugin` (no shared base trait)
2. Create `PluginRegistry` to manage both plugin types
3. Add `ScreenId::Plugin(String)` variant with flat screen navigation (direct field assignment, no stack)
4. Wire screen plugins into the app's render loop, key handler, and tick system
5. Implement lifecycle hooks (`on_enter`, `on_leave`) for screen plugins
6. Add hardcoded `J` keybinding for screen entry (AboutPlugin demo)
7. Migrate existing sidebar plugins to the new `SidebarPlugin` trait
8. Create a demo screen plugin (AboutPlugin) to verify the architecture
9. Hide sidebar when a screen plugin is active
10. Fix pre-existing proptest failure that blocks the test gate

### Out of Scope

- JIRA API integration (Phase 1)
- New sidebar plugins
- Changes to existing screen behavior
- Plugin hot-loading or dynamic discovery
- Config parsing for screen plugins (deferred to Phase 1 — AboutPlugin has no config)

## Current State

### Existing Plugin Trait

```rust
// crates/jm-tui/src/plugins/mod.rs
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

### Existing Plugins

| Plugin | Height | Timer | Keys | State |
|--------|--------|-------|------|-------|
| Clock | 4 | Yes | None | Stateless |
| Notifications | Dynamic | Yes | `c` (clear) | Mutable |
| Pomodoro | 6 | Yes | Space, +, -, r, R | Mutable |

### Existing Integration Points

- `PluginSidebar` in `sidebar.rs` — manages `Vec<Box<dyn Plugin>>`
- `App.plugins` field — `PluginSidebar` instance
- `Focus::Sidebar(usize)` — sidebar focus state
- `Action::ToggleSidebar` / `Action::FocusSidebar` — sidebar visibility
- Dashboard renders sidebar in right 22-column panel
- Tick system calls `plugins.on_tick()` every 1s

## Design

### 1. Trait Design — Two Independent Traits (No Base Trait)

**File: `crates/jm-tui/src/plugins/mod.rs`**

The old `Plugin` base trait is **dropped entirely**. `SidebarPlugin` and `ScreenPlugin` are two **independent traits with no supertrait**. Each trait is self-contained with its own `name()` and `needs_timer()` methods.

```rust
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::Frame;

/// A plugin that renders in the sidebar (small widget).
/// Independent trait — no supertrait.
pub trait SidebarPlugin {
    fn name(&self) -> &str;
    fn needs_timer(&self) -> bool { false }
    fn on_tick(&mut self) -> Vec<String> { Vec::new() }
    fn on_notify(&mut self, _message: &str) {}
    fn on_key(&mut self, _key: KeyEvent) -> bool { false }
    fn height(&self) -> u16 { 3 }
    fn render(&self, area: Rect, buf: &mut Buffer);
}

/// Actions a screen plugin can request from the app.
/// Deliberately limited — screen plugins do NOT have access to the full Action enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginAction {
    None,
    Back,
    Toast(String),
}

/// A plugin that renders as a full screen.
/// Independent trait — no supertrait.
/// Uses Frame (not Buffer) for its render signature.
pub trait ScreenPlugin {
    fn name(&self) -> &str;
    fn needs_timer(&self) -> bool { false }
    fn on_tick(&mut self) -> Vec<String> { Vec::new() }
    fn render(&self, frame: &mut Frame, area: Rect);
    fn handle_key(&mut self, key: KeyEvent) -> PluginAction;
    fn on_enter(&mut self);
    fn on_leave(&mut self);
    fn key_hints(&self) -> Vec<(&'static str, &'static str)> { Vec::new() }
}
```

**Key decisions:**

- **No base `Plugin` trait.** The old design had `Plugin` as a supertrait of both `SidebarPlugin` and `ScreenPlugin`, adding complexity for no benefit. Each trait stands alone.
- **`ScreenPlugin::render` uses `Frame`, not `Buffer`.** Screen plugins need `Frame` for full-screen rendering (e.g., setting cursor position). Sidebar plugins continue to use `Buffer` since they render into a sub-area.
- **`ScreenPlugin::handle_key` returns `PluginAction`, not `Action`.** Screen plugins have a deliberately limited action vocabulary. The app translates `PluginAction` values into internal `Action` values as needed.
- **`key_hints()` returns `Vec<(&'static str, &'static str)>` with `'static` lifetimes.** This avoids lifetime entanglement with the plugin's borrow.
- **Screen plugins manage their own modals internally.** The App's modal system (input modal, select modal, confirm modal) is NOT exposed to plugins. If a screen plugin needs modal-like behavior, it tracks that state within its own struct and renders it in its own `render()` method.

### 2. Migration Plan for Existing Plugins

Each existing plugin implements `SidebarPlugin` directly (no `Plugin` base):

```rust
// Example: ClockPlugin migration
impl SidebarPlugin for ClockPlugin {
    fn name(&self) -> &str { "clock" }
    fn needs_timer(&self) -> bool { true }
    fn height(&self) -> u16 { 4 }
    fn render(&self, area: Rect, buf: &mut Buffer) { /* unchanged */ }
}
```

**Migration is mechanical** — the old `Plugin` trait's methods move directly into `SidebarPlugin`. No behavioral changes.

### 3. PluginRegistry

**New file: `crates/jm-tui/src/plugins/registry.rs`**

Screen plugins are stored in a `Vec<Box<dyn ScreenPlugin>>` (not HashMap). Lookup is by linear scan on `name()` — the number of screen plugins will always be small.

```rust
pub struct PluginRegistry {
    pub sidebar: PluginSidebar,
    pub screens: Vec<Box<dyn ScreenPlugin>>,
}

impl PluginRegistry {
    pub fn new(config: &Config) -> Self {
        let mut sidebar_plugins: Vec<Box<dyn SidebarPlugin>> = Vec::new();
        let mut screen_plugins: Vec<Box<dyn ScreenPlugin>> = Vec::new();

        for name in &config.plugins.enabled {
            match name.as_str() {
                "clock" => sidebar_plugins.push(Box::new(ClockPlugin::new())),
                "pomodoro" => sidebar_plugins.push(Box::new(PomodoroPlugin::new(/* ... */))),
                "notifications" => sidebar_plugins.push(Box::new(NotificationsPlugin::new(/* ... */))),
                _ => {} // skip unknown
            }
        }

        // AboutPlugin is always registered (no config needed)
        screen_plugins.push(Box::new(AboutPlugin::new()));

        Self {
            sidebar: PluginSidebar::new_from(sidebar_plugins),
            screens: screen_plugins,
        }
    }

    /// Find a screen plugin by name
    pub fn get_screen(&self, name: &str) -> Option<&dyn ScreenPlugin> {
        self.screens.iter().find(|p| p.name() == name).map(|p| &**p)
    }

    pub fn get_screen_mut(&mut self, name: &str) -> Option<&mut Box<dyn ScreenPlugin>> {
        self.screens.iter_mut().find(|p| p.name() == name)
    }

    /// Tick sidebar plugins (1s interval, always ticked)
    pub fn tick_sidebar(&mut self) -> Vec<String> {
        self.sidebar.on_tick()
    }

    /// Tick the active screen plugin (250ms interval, only the active one)
    pub fn tick_screen(&mut self, name: &str) -> Vec<String> {
        if let Some(plugin) = self.get_screen_mut(name) {
            if plugin.needs_timer() {
                return plugin.on_tick();
            }
        }
        Vec::new()
    }
}
```

### 4. Screen Navigation — Flat Field, Not a Stack

**File: `crates/jm-tui/src/events.rs`**

```rust
pub enum ScreenId {
    Dashboard,
    ProjectView,
    Switch,
    Search,
    IssueBoard,
    Weekly,
    Review,
    People,
    Plugin(String),  // NEW: identifies a screen plugin by name
}
```

**CRITICAL: Navigation uses a flat `screen: ScreenId` field, NOT a stack.**

```rust
// Opening a screen plugin:
self.screen = ScreenId::Plugin(name);

// Going back — ALWAYS returns to Dashboard:
self.screen = ScreenId::Dashboard;
```

There is no `screen_stack`, no `push`, no `pop`, no `last()`. The `handle_back()` function sets `self.screen = ScreenId::Dashboard` for plugin screens (after calling `on_leave()`).

### 5. App Integration

**File: `crates/jm-tui/src/app.rs`**

Changes to the App struct:

```rust
pub struct App {
    // Replace: plugins: PluginSidebar
    // With:
    pub plugin_registry: PluginRegistry,
    // ... rest unchanged
}
```

**Rename `self.plugins` to `self.plugin_registry` across all 7 call sites in app.rs.**

#### Match Sites Requiring `ScreenId::Plugin(_)` Arms

All 9+ match/if-let sites on `ScreenId` must handle the `Plugin(_)` variant. Add wildcard arms BEFORE implementing real behavior to prevent match exhaustiveness errors:

1. **`render()` in `app.rs`** — Render the active screen plugin full-screen. **Hide sidebar** when a screen plugin is active.
2. **`handle_key()` in `app.rs`** — Delegate to the active screen plugin's `handle_key()`.
3. **`current_project()` in `app.rs`** — Return `None` for plugin screens.
4. **`targeted_project_slug()` in `app.rs`** — Return `None` for plugin screens.
5. **Help modal screen name in `app.rs`** — Use the plugin's `name()` as the screen label.
6. **`handle_select()` in `app.rs`** — No-op for plugin screens.
7. **`handle_back()` in `app.rs`** — Call `on_leave()`, then set `self.screen = ScreenId::Dashboard`.
8. **`handle_start_work()` in `app.rs`** — No-op for plugin screens.
9. **`get_hints()` in `keyhints.rs`** — Return the plugin's `key_hints()`.

#### Render loop

When `self.screen` is `ScreenId::Plugin(name)`:

```rust
ScreenId::Plugin(ref name) => {
    // Sidebar is HIDDEN for screen plugins — render full area
    if let Some(plugin) = self.plugin_registry.get_screen(name) {
        plugin.render(frame, main_area);
    }
}
```

#### Key handling

When a screen plugin is active:

```rust
ScreenId::Plugin(ref name) => {
    // Clone name to avoid borrow conflict (see Borrow Checker Note below)
    let name = name.clone();
    if let Some(plugin) = self.plugin_registry.get_screen_mut(&name) {
        let action = plugin.handle_key(key);
        match action {
            PluginAction::None => {},
            PluginAction::Back => self.handle_back(),
            PluginAction::Toast(msg) => { /* show toast */ },
        }
    }
}
```

#### Screen lifecycle

```rust
// When opening a screen plugin
Action::OpenPlugin(name) => {
    if let Some(plugin) = self.plugin_registry.get_screen_mut(&name) {
        plugin.on_enter();
        self.screen = ScreenId::Plugin(name);
    }
}

// handle_back() — when closing a plugin screen
fn handle_back(&mut self) {
    // Clone screen to avoid borrow conflict
    let current = self.screen.clone();
    if let ScreenId::Plugin(name) = current {
        if let Some(plugin) = self.plugin_registry.get_screen_mut(&name) {
            plugin.on_leave();
        }
    }
    self.screen = ScreenId::Dashboard;
}
```

#### Borrow Checker Note

Several methods need the **clone-first pattern**: when `self.screen` is `ScreenId::Plugin(name)`, the `name` borrows `self.screen`, but we also need `&mut self.plugin_registry`. The solution is to clone `name` (or clone the entire `ScreenId`) before taking the mutable borrow:

```rust
// BAD — name borrows self.screen, then &mut self borrows all of self
if let ScreenId::Plugin(ref name) = self.screen {
    self.plugin_registry.get_screen_mut(name); // ERROR: &mut self conflicts
}

// GOOD — clone releases the borrow on self.screen
let screen = self.screen.clone();
if let ScreenId::Plugin(name) = screen {
    self.plugin_registry.get_screen_mut(&name); // OK: no conflicting borrow
}
```

This pattern applies to `handle_back()` and `handle_key()` at minimum.

### 6. Screen Plugin Ticking

**Sidebar plugins** tick at the existing **1s interval** and are always ticked.

**Screen plugins** tick at a **250ms interval** and **only the active screen plugin** is ticked. Inactive screen plugins receive no ticks.

```rust
// In the app's tick handler:
// Always tick sidebar (1s interval)
let notifications = self.plugin_registry.tick_sidebar();

// Tick active screen plugin only (250ms interval)
if let ScreenId::Plugin(ref name) = self.screen {
    let name = name.clone();
    let screen_notifications = self.plugin_registry.tick_screen(&name);
    notifications.extend(screen_notifications);
}
```

### 7. Plugin Keybinding — Hardcoded `J`

Screen plugins are opened via hardcoded keybindings. For Phase 0, the AboutPlugin uses **uppercase `J`** (Shift+J), NOT Ctrl+J:

```rust
// In dashboard key handling
KeyCode::Char('J') => {
    return Action::OpenPlugin("about".to_string());
}
```

A configurable keybinding system is deferred to a future phase.

### 8. PluginSidebar Refactor

The existing `PluginSidebar` changes minimally:
- Internal storage changes from `Vec<Box<dyn Plugin>>` to `Vec<Box<dyn SidebarPlugin>>`
- `new()` factory is replaced by `new_from(plugins: Vec<Box<dyn SidebarPlugin>>)` — registry handles instantiation
- All rendering and key handling logic is unchanged

### 9. Action Enum Extensions

```rust
pub enum Action {
    // ... existing actions ...
    OpenPlugin(String),   // NEW: open a screen plugin by name
}
```

### 10. Key Hints Integration

Screen plugins provide key hints via `key_hints()`:

```rust
fn key_hints(&self) -> Vec<(&'static str, &'static str)> {
    vec![
        ("Esc", "back"),
    ]
}
```

The `keyhints.rs` module renders these when a plugin screen is active. The `get_hints()` function matches `ScreenId::Plugin(ref name)` and returns the plugin's hints.

## Demo Screen Plugin

To verify the architecture, implement a minimal `AboutPlugin`:

```rust
pub struct AboutPlugin {
    version: String,
}

impl ScreenPlugin for AboutPlugin {
    fn name(&self) -> &str { "about" }

    fn render(&self, frame: &mut Frame, area: Rect) {
        // Render "jm v0.1.0" centered, with build info
    }

    fn handle_key(&mut self, key: KeyEvent) -> PluginAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => PluginAction::Back,
            _ => PluginAction::None,
        }
    }

    fn on_enter(&mut self) {
        // No-op for About screen
    }

    fn on_leave(&mut self) {
        // No-op for About screen
    }

    fn key_hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Esc", "back")]
    }
}
```

Note: `AboutPlugin` implements `ScreenPlugin` directly — there is no `Plugin` base trait to implement.

## Acceptance Criteria (Phase 0 Gate)

### Regression

- [ ] Clock plugin renders correctly in sidebar
- [ ] Notifications plugin receives and displays messages, clears on 'c', reminders fire
- [ ] Pomodoro plugin starts/pauses/resets, transitions between states, emits notifications
- [ ] Sidebar focus (Tab) works: navigate between plugins, Esc to unfocus
- [ ] Sidebar toggle works
- [ ] Toast notifications from plugin ticks still appear
- [ ] All existing `cargo test` pass (including proptest fix from Task 0)

### New Functionality

- [ ] `SidebarPlugin` and `ScreenPlugin` are independent traits with no supertrait
- [ ] `PluginRegistry` manages both plugin types
- [ ] `ScreenId::Plugin(String)` variant added and handled via flat `screen` field (no stack)
- [ ] Demo `AboutPlugin` can be opened via `J` keybinding
- [ ] Demo screen renders correctly (full screen area)
- [ ] **Sidebar is hidden when AboutPlugin screen is active**
- [ ] Demo screen handles keys (Esc returns to dashboard)
- [ ] Demo screen lifecycle: `on_enter` called on open, `on_leave` called on close
- [ ] **`handle_back()` calls `on_leave()` for plugin screens**
- [ ] Key hints render correctly for the demo screen
- [ ] Demo screen does not interfere with sidebar plugins

### Code Quality

- [ ] No `unsafe` code introduced
- [ ] No new `unwrap()` on fallible operations
- [ ] Existing sidebar plugin code changes are minimal (trait split only)
- [ ] New code has doc comments on public items

## Tasks

### Task 0: Fix pre-existing proptest failure

Fix `prop_project_name_with_yaml_special_chars` proptest failure. This is a pre-existing bug unrelated to the plugin rewrite, but it blocks the "all tests pass" gate.

### Task 1: Define new trait hierarchy in `plugins/mod.rs`

Drop the `Plugin` base trait. Define `SidebarPlugin` and `ScreenPlugin` as two independent traits. Define the `PluginAction` enum (`None`, `Back`, `Toast(String)`).

### Task 2: Migrate ClockPlugin to `SidebarPlugin`

Implement `SidebarPlugin` directly (no `Plugin` impl needed).

### Task 3: Migrate NotificationsPlugin to `SidebarPlugin`

Implement `SidebarPlugin` directly (no `Plugin` impl needed).

### Task 4: Migrate PomodoroPlugin to `SidebarPlugin`

Implement `SidebarPlugin` directly (no `Plugin` impl needed).

### Task 5: Refactor `PluginSidebar` to use `Vec<Box<dyn SidebarPlugin>>`

Change internal storage from `Vec<Box<dyn Plugin>>` to `Vec<Box<dyn SidebarPlugin>>`. Replace `new()` with `new_from()`. This must happen BEFORE creating PluginRegistry since the registry depends on the refactored sidebar.

### Task 6: Create `PluginRegistry` in `plugins/registry.rs`

Store screen plugins in `Vec<Box<dyn ScreenPlugin>>`. Provide `get_screen()` / `get_screen_mut()` by name (linear scan). Separate tick methods for sidebar (1s) and active screen (250ms).

### Task 7: Add `ScreenId::Plugin(String)` to `events.rs`

Add the variant. Derive `Clone` on `ScreenId` if not already present (needed for the clone-first borrow pattern).

### Task 8: Add `ScreenId::Plugin(_)` wildcard arms to ALL match sites

Before implementing real behavior, add wildcard arms to all 9+ match sites to prevent exhaustiveness errors:
1. `render()` in `app.rs`
2. `handle_key()` in `app.rs`
3. `current_project()` in `app.rs`
4. `targeted_project_slug()` in `app.rs`
5. Help modal screen name in `app.rs`
6. `handle_select()` in `app.rs`
7. `handle_back()` in `app.rs`
8. `handle_start_work()` in `app.rs`
9. `get_hints()` in `keyhints.rs`

### Task 9: Add `Action::OpenPlugin(String)` to `events.rs`

### Task 10: Rename `self.plugins` to `self.plugin_registry` in `app.rs`

Update all 7 call sites. Replace `PluginSidebar` with `PluginRegistry` in the App struct.

### Task 11: Handle `Action::OpenPlugin` in `update()`

Wire the action: look up the screen plugin, call `on_enter()`, set `self.screen = ScreenId::Plugin(name)`.

### Task 12: Modify `handle_back()` to call `on_leave()` for plugin screens

Use the clone-first pattern to avoid borrow conflicts. Call `on_leave()` on the active screen plugin, then set `self.screen = ScreenId::Dashboard`.

### Task 13: Wire screen plugin rendering into `app.rs`

When `self.screen` is `ScreenId::Plugin(name)`, render the plugin full-screen. **Hide sidebar** (do not split layout for sidebar panel).

### Task 14: Wire screen plugin key handling into `app.rs`

Delegate to plugin's `handle_key()`, translate `PluginAction` into app behavior. Use clone-first pattern.

### Task 15: Wire key hints for screen plugins into `keyhints.rs`

Match `ScreenId::Plugin(ref name)` in `get_hints()`, return the plugin's `key_hints()`.

### Task 16: Implement `AboutPlugin` demo

Create `plugins/about.rs`. Implement `ScreenPlugin` directly (no base trait). Render version info centered. Handle Esc/q to return.

### Task 17: Register `AboutPlugin` in `PluginRegistry`

Always register (no config needed).

### Task 18: Add `J` keybinding for `AboutPlugin`

Hardcoded uppercase `J` in dashboard key handling. Returns `Action::OpenPlugin("about".to_string())`.

### Task 19: Write unit tests for `PluginRegistry` and `AboutPlugin`

Test plugin lookup by name, tick behavior, `PluginAction` handling.

### Task 20: Run full test suite — all tests pass

Including the proptest fix from Task 0.

### Task 21: Manual testing — all sidebar plugins work, About screen works

Verify sidebar is hidden when About screen is active. Verify `on_leave()` is called on back.
