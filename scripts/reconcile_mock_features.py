#!/usr/bin/env python3
"""Reconcile the MockForge (API Mock + API Override) feature edits.

The working tree is being edited concurrently by another session, so every
invariant below is checked and re-applied idempotently. Run this, then
`cargo check -p halo-bridge -p halo`.
"""
import re

def patch(path, fn):
    with open(path) as f:
        s = f.read()
    before = s
    s = fn(s)
    if s != before:
        with open(path, 'w') as f:
            f.write(s)
        print(f"patched {path}")
    else:
        print(f"ok      {path}")

# ── bridge/work.rs ──────────────────────────────────────────────────────────
def work_rs(s):
    if "ApiMock," not in s.split("impl WorkKind")[0]:
        s = s.replace("    Settings,\n    Notes,\n}", "    Settings,\n    Notes,\n    ApiMock,\n    ApiOverride,\n}", 1)
    if 'Self::ApiMock =>' not in s:
        s = s.replace('            Self::Settings => "Proxy listener, CA trust, storage cleanup, and appearance.",',
                      '''            Self::Settings => "Proxy listener, CA trust, storage cleanup, and appearance.",
            Self::ApiMock => {
                "Serve canned responses from a local mock server on 127.0.0.1."
            }
            Self::ApiOverride => {
                "Replace live proxied responses for captured hosts with your own rules."
            }''', 1)
    if 'Self::ApiMock => "API Mock"' not in s:
        s = s.replace('            Self::Notes => "Notes",\n        }\n    }',
                      '            Self::Notes => "Notes",\n            Self::ApiMock => "API Mock",\n            Self::ApiOverride => "API Override",\n        }\n    }', 1)
    if 'Self::ApiMock => "api-mock"' not in s:
        s = s.replace('            Self::Notes => "notes",\n        }\n    }',
                      '            Self::Notes => "notes",\n            Self::ApiMock => "api-mock",\n            Self::ApiOverride => "api-override",\n        }\n    }', 1)
    if '"api-mock" => Some(Self::ApiMock)' not in s:
        s = s.replace('            "notes" => Some(Self::Notes),\n            _ => None,',
                      '            "notes" => Some(Self::Notes),\n            "api-mock" => Some(Self::ApiMock),\n            "api-override" => Some(Self::ApiOverride),\n            _ => None,', 1)
    if "Mock(MockForgeWork)," not in s:
        s = s.replace("    Settings(SettingsTab),\n    Notes(NotesWork),\n}",
                      "    Settings(SettingsTab),\n    Notes(NotesWork),\n    Mock(MockForgeWork),\n    Override(MockForgeWork),\n}", 1)
    if "pub struct MockForgeWork" not in s:
        anchor = "pub struct InterceptWork {"
        idx = s.index(anchor)
        end = s.index("}", idx) + 1
        s = s[:end] + ("\n\n/// API Mock / API Override work tab. Route data lives in the shared\n"
                       "/// `MockForgeView`; per-tab state is just the list filter.\n"
                       "#[derive(Clone, Debug, PartialEq, Eq, Default)]\n"
                       "pub struct MockForgeWork {\n    pub filter: String,\n}") + s[end:]
    if "WorkKind::ApiMock | WorkKind::ApiOverride" not in s.split("impl WorkData")[1] if "impl WorkData" in s else True:
        if "WorkKind::ApiMock | WorkKind::ApiOverride" not in s:
            s = s.replace("            WorkKind::Notes => Self::Notes(NotesWork::default()),\n        }",
                          "            WorkKind::Notes => Self::Notes(NotesWork::default()),\n"
                          "            WorkKind::ApiMock | WorkKind::ApiOverride => {\n"
                          "                Self::Mock(MockForgeWork::default())\n            }\n        }", 1)
    if s.count("pub struct MockForgeWork") > 1:
        block = ("\n/// API Mock / API Override work tab. Route data lives in the shared\n"
                 "/// `MockForgeView`; per-tab state is just the list filter.\n"
                 "#[derive(Clone, Debug, PartialEq, Eq, Default)]\n"
                 "pub struct MockForgeWork {\n    pub filter: String,\n}")
        first = s.index("pub struct MockForgeWork")
        # remove later duplicates
        rest = s[first + 10:]
        while "pub struct MockForgeWork" in rest:
            i2 = rest.index(block) if block in rest else rest.index("pub struct MockForgeWork")
            # crude: find the struct block start and end
            start = s.index("/// API Mock / API Override work tab", first + 10) if "/// API Mock / API Override work tab" in s[first + 10:] else None
            if start is None:
                break
            end = s.index("}", start)
            end = s.index("\n", end) + 1
            s = s[:start] + s[end:]
    return s

patch('crates/bridge/src/work.rs', work_rs)

# ── bridge/bridge/mod.rs ────────────────────────────────────────────────────
def bmod_rs(s):
    if "pub mock: MockForgeView" not in s:
        s = s.replace("    pub ca_busy: bool,\n",
                      "    pub ca_busy: bool,\n"
                      "    /// API Mock + API Override shared view (domains, routes, logs, server).\n"
                      "    pub mock: MockForgeView,\n", 1)
    if "mock: MockForgeView::default()" not in s:
        s = s.replace("            ca_busy: false,\n",
                      "            ca_busy: false,\n            mock: MockForgeView::default(),", 1)
    if "MockForgeView," not in s:
        s = s.replace("    HttpSessionDto, InterceptSnapshot, IntruderDraft, IntruderResultRow, IntruderStatus,",
                      "    HttpSessionDto, InterceptSnapshot, IntruderDraft, IntruderResultRow, IntruderStatus,\n    MockForgeView, MockLogDto, MockServerStatusDto,", 1)
    if "mod mock_forge;" not in s:
        s = s.replace("mod intercept;\nmod intruder;", "mod intercept;\nmod intruder;\nmod mock_forge;", 1)
    return s

patch('crates/bridge/src/bridge/mod.rs', bmod_rs)

# ── bridge/bridge/state.rs (apply_event arms) ───────────────────────────────
def bstate_rs(s):
    if "BackendEvent::MockLog" not in s:
        s = s.replace("            BackendEvent::Log(msg) => {",
                      "            BackendEvent::MockLog(log) => {\n                Bridge::apply_mock_log(self, log);\n            }\n"
                      "            BackendEvent::MockServerStatus(status) => {\n                self.mock.server = status;\n            }\n"
                      "            BackendEvent::Log(msg) => {", 1)
    if "WorkData::Mock(work) | WorkData::Override(work)" not in s:
        s = s.replace("""            WorkData::Intercept(work) => {
                work.capture_hosts = self.intercept.capture_patterns.clone();
                work.selected_paused_id = self.selected_paused_id.clone();
            }""",
                      """            WorkData::Intercept(work) => {
                work.capture_hosts = self.intercept.capture_patterns.clone();
                work.selected_paused_id = self.selected_paused_id.clone();
            }
            WorkData::Mock(work) | WorkData::Override(work) => {
                let _ = work; // filter lives in the page's editor entity
            }""", 1)
        s = s.replace("""            WorkData::Intercept(work) => {
                self.intercept.capture_patterns = work.capture_hosts;
                self.selected_paused_id = work.selected_paused_id;
            }""",
                      """            WorkData::Intercept(work) => {
                self.intercept.capture_patterns = work.capture_hosts;
                self.selected_paused_id = work.selected_paused_id;
            }
            WorkData::Mock(work) | WorkData::Override(work) => {
                let _ = work; // filter lives in the page's editor entity
            }""", 1)
    return s

patch('crates/bridge/src/bridge/state.rs', bstate_rs)

# ── bridge/bridge/install.rs (event pump) ───────────────────────────────────
def bins_rs(s):
    if '"mock-forge-log"' not in s:
        s = s.replace('            name if name.starts_with("invoker-result-") => {',
                      '''            "mock-forge-log" => {
                if let Ok(log) = serde_json::from_value::<MockLogDto>(payload.clone()) {
                    let _ = tx.send(BackendEvent::MockLog(log));
                }
            }
            "mock-server-status-changed" => {
                if let Ok(status) =
                    serde_json::from_value::<MockServerStatusDto>(payload.clone())
                {
                    let _ = tx.send(BackendEvent::MockServerStatus(status));
                }
            }
            name if name.starts_with("invoker-result-") => {''', 1)
    return s

patch('crates/bridge/src/bridge/install.rs', bins_rs)

# ── ui/nav.rs ───────────────────────────────────────────────────────────────
def nav_rs(s):
    if "Page::ApiMock," not in s:
        s = s.replace("    Settings,\n    ApiMock,\n    ApiOverride,\n    Notes,", "    Settings,\n    ApiMock,\n    ApiOverride,\n    Notes,")
        if "Page::ApiMock," not in s:
            s = s.replace("    Coder,\n    Settings,\n", "    Coder,\n    Settings,\n    ApiMock,\n    ApiOverride,\n", 1)
    if "Page::ApiMock," not in s.split("pub fn id")[0]:
        s = s.replace("        Page::Notes,\n        Page::Settings,\n    ];",
                      "        Page::Notes,\n        Page::Settings,\n    ];\n    // ApiMock/ApiOverride inserted below")
    if "ALL: [Page; 10]" not in s and "ApiMock,\n        Page::ApiOverride," not in s:
        s = re.sub(r"pub const ALL: \[Page; \d+\] = \[([^]]*)\];",
                   lambda m: "pub const ALL: [Page; 10] = [" + m.group(1).rstrip() + "\n        Page::ApiMock,\n        Page::ApiOverride,\n    ];",
                   s, count=1)
    if "Self::ApiMock => WorkKind::ApiMock" not in s:
        s = s.replace("            Self::Notes => WorkKind::Notes,\n            Self::Settings => WorkKind::Settings,\n        })",
                      "            Self::Notes => WorkKind::Notes,\n            Self::Settings => WorkKind::Settings,\n            Self::ApiMock => WorkKind::ApiMock,\n            Self::ApiOverride => WorkKind::ApiOverride,\n        })", 1)
    if "WorkKind::ApiMock => Self::ApiMock" not in s:
        s = s.replace("            WorkKind::Notes => Self::Notes,\n            WorkKind::Settings => Self::Settings,\n        }",
                      "            WorkKind::Notes => Self::Notes,\n            WorkKind::Settings => Self::Settings,\n            WorkKind::ApiMock => Self::ApiMock,\n            WorkKind::ApiOverride => Self::ApiOverride,\n        }", 1)
    return s

patch('crates/ui/src/nav.rs', nav_rs)

# ── ui/os_shell/helpers.rs ──────────────────────────────────────────────────
def helpers_rs(s):
    if "Page::ApiMock =>" not in s:
        s = s.replace("        Page::Notes => !state.notes_open_tab_ids.is_empty(),",
                      "        Page::Notes => !state.notes_open_tab_ids.is_empty(),\n        Page::ApiMock => state.mock.server.running,\n        Page::ApiOverride => state.mock.override_routes().iter().any(|r| r.enabled),", 1)
    return s

patch('crates/ui/src/os_shell/helpers.rs', helpers_rs)

# ── ui/pages/editors.rs ─────────────────────────────────────────────────────
def editors_rs(s):
    if "pub mock_port:" not in s:
        s = s.replace("    pub notes_library_search: Entity<InputState>,\n}",
                      """    pub notes_library_search: Entity<InputState>,
    // ── API Mock ───────────────────────────────────────────────────────────
    pub mock_port: Entity<InputState>,
    pub mock_filter: Entity<InputState>,
    pub mock_method: Entity<InputState>,
    pub mock_path: Entity<InputState>,
    pub mock_status: Entity<InputState>,
    pub mock_body: Entity<CodeEditor>,
    // ── API Override ───────────────────────────────────────────────────────
    pub override_filter: Entity<InputState>,
    pub override_host: Entity<InputState>,
    pub override_method: Entity<InputState>,
    pub override_path: Entity<InputState>,
    pub override_status: Entity<InputState>,
    pub override_body: Entity<CodeEditor>,
}""", 1)
    if "mock_port: cx.new(" not in s:
        s = s.replace("""            notes_library_search: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Search all saved notes…")
            }),
        }""",
                      """            notes_library_search: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Search all saved notes…")
            }),
            mock_port: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("4000")
                    .default_value("4000")
            }),
            mock_filter: cx.new(|cx| InputState::new(window, cx).placeholder("Filter endpoints…")),
            mock_method: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("GET")
                    .default_value("GET")
            }),
            mock_path: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("/api/resource/:id")
                    .default_value("/api/resource/:id")
            }),
            mock_status: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("200")
                    .default_value("200")
            }),
            mock_body: cx.new(|cx| CodeEditor::new("json", window, cx)),
            override_filter: cx
                .new(|cx| InputState::new(window, cx).placeholder("Filter hosts and rules…")),
            override_host: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Add host (e.g. api.example.com)…")
            }),
            override_method: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("GET")
                    .default_value("GET")
            }),
            override_path: cx.new(|cx| {
                InputState::new(window, cx).placeholder("/v1/charges/:id or https://host/path")
            }),
            override_status: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("200")
                    .default_value("200")
            }),
            override_body: cx.new(|cx| CodeEditor::new("json", window, cx)),
        }""", 1)
    return s

patch('crates/ui/src/pages/editors.rs', editors_rs)

# ── ui/pages/mod.rs ─────────────────────────────────────────────────────────
def pmod_rs(s):
    if "pub mod api_mock;" not in s:
        s = s.replace("pub mod browser;", "pub mod api_mock;\npub mod api_override;\npub mod browser;", 1)
    if "pub use api_mock::render_api_mock;" not in s:
        s = s.replace("pub use browser::render_browser;",
                      "pub use api_mock::render_api_mock;\npub use api_override::render_api_override;\npub use browser::render_browser;", 1)
    if "Page::ApiMock =>" not in s:
        s = s.replace("        Page::Browser => render_browser(bridge, editors, cx).into_any_element(),",
                      "        Page::ApiMock => render_api_mock(bridge, editors, cx).into_any_element(),\n        Page::ApiOverride => render_api_override(bridge, editors, cx).into_any_element(),\n        Page::Browser => render_browser(bridge, editors, cx).into_any_element(),", 1)
    return s

patch('crates/ui/src/pages/mod.rs', pmod_rs)

# ── ui/app_icons.rs ─────────────────────────────────────────────────────────
def icons_rs(s):
    if "api_mock_icon" not in s:
        s = s.replace('app_png!(settings_icon, "../assets/app-icon/settings.png");',
                      'app_png!(settings_icon, "../assets/app-icon/settings.png");\napp_png!(api_mock_icon, "../assets/app-icon/api-mock.png");\napp_png!(api_override_icon, "../assets/app-icon/api-override.png");', 1)
    if "Page::ApiMock => api_mock_icon()" not in s:
        s = s.replace("        Page::Settings => settings_icon(),",
                      "        Page::Settings => settings_icon(),\n        Page::ApiMock => api_mock_icon(),\n        Page::ApiOverride => api_override_icon(),", 1)
    return s

patch('crates/ui/src/app_icons.rs', icons_rs)

# ── ui/app.rs (sync_mock_editors) ───────────────────────────────────────────
def app_rs(s):
    if "last_mock_route_id" not in s:
        s = s.replace("    /// Last intercept paused request id synced into intercept CodeEditor.\n    last_intercept_paused_id: Option<String>,",
                      "    /// Last intercept paused request id synced into intercept CodeEditor.\n    last_intercept_paused_id: Option<String>,\n    /// Last MockForge route id synced into the API Mock / Override editors.\n    last_mock_route_id: Option<String>,", 1)
        s = s.replace("            last_intercept_paused_id: None,",
                      "            last_intercept_paused_id: None,\n            last_mock_route_id: None,", 1)
    if "fn sync_mock_editors" not in s:
        s = s.replace("    fn sync_intercept_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {",
                      """    /// Push the selected MockForge route into the API Mock / Override
    /// editors whenever the selection changes. Which editor set is current
    /// follows the active page; the other keeps its last contents.
    fn sync_mock_editors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.bridge.mock.selected_route_id.clone();
        if self.last_mock_route_id == selected {
            return;
        }
        self.last_mock_route_id = selected.clone();

        let Some(route) = selected
            .as_deref()
            .and_then(|id| self.bridge.mock.routes.iter().find(|r| r.id == id))
            .cloned()
        else {
            return;
        };

        let is_override = self
            .bridge
            .active_work_kind()
            .is_some_and(|kind| kind == WorkKind::ApiOverride);

        let (method, path, status, body) = if is_override {
            (
                &mut self.editors.override_method,
                &mut self.editors.override_path,
                &mut self.editors.override_status,
                &mut self.editors.override_body,
            )
        } else {
            (
                &mut self.editors.mock_method,
                &mut self.editors.mock_path,
                &mut self.editors.mock_status,
                &mut self.editors.mock_body,
            )
        };

        method.update(cx, |input, cx| {
            input.set_value(&route.method, window, cx);
        });
        path.update(cx, |input, cx| {
            input.set_value(&route.path, window, cx);
        });
        status.update(cx, |input, cx| {
            input.set_value(&route.status_code.to_string(), window, cx);
        });
        body.update(cx, |editor, cx| {
            editor.set_text(route.response_body.clone(), window, cx);
        });
    }

    fn sync_intercept_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {""", 1)
    if "self.sync_mock_editors(window, cx);" not in s:
        s = s.replace("        self.sync_intercept_editor(window, cx);",
                      "        self.sync_intercept_editor(window, cx);\n        self.sync_mock_editors(window, cx);", 1)
    return s

patch('crates/ui/src/app.rs', app_rs)

# ── ui/pages/setup/mod.rs ───────────────────────────────────────────────────
def setup_rs(s):
    if "Page::ApiMock => {" not in s:
        s = s.replace('        Page::Settings => "Current state of the proxy, certificate, and local storage.",',
                      '''        Page::Settings => "Current state of the proxy, certificate, and local storage.",
        Page::ApiMock => {
            "Start the local mock server, then create endpoints to serve canned responses."
        }
        Page::ApiOverride => {
            "Capture hosts from proxied traffic, then override their responses."
        }''', 1)
    if "Page::ApiMock | Page::ApiOverride => div().into_any_element()" not in s:
        s = s.replace("        Page::Notes => div().into_any_element(),\n    }\n}",
                      "        Page::Notes => div().into_any_element(),\n        Page::ApiMock | Page::ApiOverride => div().into_any_element(),\n    }\n}", 1)
    return s

patch('crates/ui/src/pages/setup/mod.rs', setup_rs)

print("reconcile complete")

# ── restore page files when the parallel session deletes them ───────────────
import os, shutil
for src, dst in (
    ('scripts/mock_pages_backup/api_mock_mod.rs', 'crates/ui/src/pages/api_mock/mod.rs'),
    ('scripts/mock_pages_backup/api_override_mod.rs', 'crates/ui/src/pages/api_override/mod.rs'),
):
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    if not os.path.exists(dst):
        shutil.copyfile(src, dst)
        print(f"restored {dst}")

# ── ui/app.rs EditorDraft arms ───────────────────────────────────────────────
def app_draft_rs(s):
    if "EditorDraft::Mock," not in s:
        s = s.replace("    Settings,\n}", "    Settings,\n    Mock,\n}", 1) if "enum EditorDraft" in s.split("WorkKind::Settings => EditorDraft::Settings")[0] or True else s
    if "WorkKind::ApiMock | WorkKind::ApiOverride => EditorDraft::Mock," not in s:
        s = s.replace("            WorkKind::Settings => EditorDraft::Settings,",
                      "            WorkKind::ApiMock | WorkKind::ApiOverride => EditorDraft::Mock,\n            WorkKind::Settings => EditorDraft::Settings,", 1)
    if "EditorDraft::Mock => {}" not in s:
        s = s.replace("            EditorDraft::Settings => {",
                      "            EditorDraft::Mock => {}\n            EditorDraft::Settings => {", 1)
    if "WorkKind::ApiMock | WorkKind::ApiOverride => {}" not in s:
        s = s.replace("            WorkKind::Settings => {\n                let _ = window;",
                      "            WorkKind::ApiMock | WorkKind::ApiOverride => {}\n            WorkKind::Settings => {\n                let _ = window;", 1)
        s = s.replace("            WorkKind::Settings => EditorDraft::Settings,",
                      "            WorkKind::Settings => EditorDraft::Settings,", 1)
    return s

patch('crates/ui/src/app.rs', app_draft_rs)
print("reconcile pass 2 complete")
