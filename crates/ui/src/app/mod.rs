//! Root window — desktop, tool windows, and the bottom start bar.

use std::collections::HashMap;
use std::time::Duration;

use crate::actions::{
    CloseCoderProject, CoderQuickOpenClose, CoderQuickOpenDown, CoderQuickOpenUp,
    CommitSourceControl, HideApp, HideOtherApps, NewCoderProject, OpenCoderFolder, OpenCoderRecent,
    OpenCoderSettings, QuitApp, SaveCoderFile, ShowAllApps, ToggleCoderQuickOpen,
    ToggleCoderTerminal, ToggleCoderWordWrap, ToggleDebugger, ToggleProxy, ToggleTheme,
    TriggerCoderCompletions,
};
use crate::app_icons;
use crate::app_menu;
use crate::components;
use crate::nav::Page;
use crate::os_shell::{self, DesktopWidget, Mockup};
use crate::pages::editors::{REPEATER_ENV_NONE, ToolEditors};
use crate::pages::repeater::{
    headers_map_to_raw, join_scripts, response_editor_text, split_raw_http,
};
use crate::theme;
use crate::toast;
use crate::windows;
use celestia_ui::components::VariableInput;
use celestia_ui::components::composite::window::{
    self as app_window, CloseWindow, MinimizeWindow, NewWindow, ShowHelp, ToggleFullScreen,
    WindowChrome, ZoomWindow,
};
use celestia_ui::components::primitive::input::{InputEvent, InputState, TabSize};
use celestia_ui::components::primitive::select::{SelectEvent, SelectOption};
use celestia_ui::components::primitive::textarea::TextareaState;
use celestia_ui::theme::ActiveTheme as _;
use chrono::Local;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{
    Bridge, BridgeState, CoderSettings, CoderTerminalKind, SettingsTab, WorkData, WorkKind,
    format_raw_request, format_raw_response,
};
use halo_debugger::{
    DebugRegionMeta, DebuggerState, DebuggerTab, clear_regions, region_bounds_sensor,
    register_region, render_debugger, select_region_at, set_selected,
};

#[derive(Clone, Debug)]
pub(super) enum EditorDraft {
    Http {
        search: String,
    },
    Intercept {
        raw: String,
        scope: String,
    },
    Repeater {
        method: String,
        url: String,
        request: String,
        scripts: String,
        env: String,
        response: String,
        tree_filter: String,
    },
    Intruder {
        method: String,
        url: String,
        headers: String,
        body: String,
        payloads: String,
        grep_keyword: String,
        grep_regex: String,
        session_token: String,
        session_header: String,
    },
    Crawler {
        target: String,
        exclude: String,
        log_filter: String,
    },
    Coder,
    SourceControl,
    Agent,
    Notes {
        content: String,
        search: String,
        rename: String,
        library_search: String,
    },
    Settings,
    Mock,
    Jwt,
    PortScanner,
    Comparer,
    FileExplorer,
    R2Storage,
    Wordlists,
    SplitView,
    Doctor,
    Diagram,
}

pub struct HaloApp {
    pub(super) focus_handle: FocusHandle,
    pub(super) page: Page,
    /// Primary desktop shell: work-tab strip + dock. Satellite tool windows: false.
    pub(super) follows_work_tabs: bool,
    /// Start menu flyout above the dock.
    pub(crate) launcher_open: bool,
    /// Notification event-log flyout above the dock (bell beside the clock).
    pub(crate) notifications_open: bool,
    /// `debug_logs.len()` when the notification panel was last opened/cleared.
    pub(super) notifications_seen: usize,
    /// Session widget stack order on the desktop.
    pub(crate) widget_order: Vec<DesktopWidget>,
    /// Widget currently being dragged (for opacity).
    pub(crate) dragging_widget: Option<DesktopWidget>,
    /// Widget under the pointer during a drag (drop highlight).
    pub(crate) widget_drop_target: Option<DesktopWidget>,
    /// Dock clock — `HH:MM`.
    pub(crate) clock_time: SharedString,
    /// Dock clock — short date under the time.
    pub(crate) clock_date: SharedString,
    pub(super) bridge: BridgeState,
    pub(super) editors: ToolEditors,
    /// Last response text pushed into the repeater CodeEditor (avoid rewrite loops).
    pub(super) last_response_text: Option<String>,
    /// Last history detail id synced into the history CodeEditors.
    pub(super) last_history_detail_id: Option<String>,
    /// Last loaded forge endpoint id synced into request editors.
    pub(super) last_forge_endpoint_id: Option<String>,
    /// Last active context id synced into env vars editor.
    pub(super) last_forge_context_id: Option<String>,
    /// Fingerprint of contexts + active id last pushed into the env Select.
    pub(super) last_forge_env_fingerprint: Option<String>,
    /// Fingerprint of collections + active id last pushed into the collection Select.
    pub(super) last_forge_collection_fingerprint: Option<String>,
    /// Last intercept paused request id synced into intercept CodeEditor.
    pub(super) last_intercept_paused_id: Option<String>,
    /// Last MockForge route id synced into the API Mock / Override editors.
    pub(super) last_mock_route_id: Option<String>,
    /// Last route id whose chaos inputs were hydrated.
    pub(super) last_chaos_route: Option<String>,
    /// Last JWT token text fed into the bridge decoder (live decode).
    pub(super) last_jwt_token: String,
    /// Last Comparer editor texts pushed into the bridge (live diff).
    pub(super) last_comparer_a: String,
    pub(super) last_comparer_b: String,
    /// Last File Explorer search strings.
    pub(super) last_fe_search: String,
    pub(super) last_wl_search: String,
    last_r2_search: String,
    /// R2 settings form has been filled from saved credentials once.
    r2_form_seeded: bool,
    /// Last Bridge `last_message` already shown as a toast (dedupe + skip startup).
    pub(super) last_toasted_message: Option<String>,
    /// Last Bridge `last_error` already shown as a toast (dedupe + skip startup).
    pub(super) last_toasted_error: Option<String>,
    /// Editor text captured per work tab so switching tabs restores drafts.
    pub(super) editor_drafts: HashMap<String, EditorDraft>,
    pub(super) last_work_tab_id: Option<String>,
    pub(super) last_work_kind: Option<WorkKind>,
    /// Last Coder project root whose buffers were pushed into the editors.
    pub(super) last_coder_root: Option<String>,
    /// Last editor prefs pushed into the Coder `CodeEditor` widgets.
    pub(super) last_coder_editor_settings: Option<CoderSettings>,
    pub(super) last_notes_active_id: Option<String>,
    /// In-app DevTools (Elements / Network / Storage / Logs).
    pub(super) debugger: DebuggerState,
    /// Design-review surface (`HALO_MOCKUP=window`): replaces the whole window.
    pub(super) mockup: Option<Mockup>,
    /// Set once the shell marks have been pre-rasterized off the UI thread.
    pub(super) icons_warmed: bool,
}

mod actions;
mod construction;
mod drafts;
mod nav;
mod render;
mod sync;
mod work_tabs;

use render::*;
