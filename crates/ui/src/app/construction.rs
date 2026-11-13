//! Construction, workspace restore, and desktop-widget drag plumbing.

use super::*;

impl HaloApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut app = Self::open(Page::Desktop, true, window, cx);
        // Re-open the last work-tab app from the persisted tab strip.
        app.follow_active_work(cx);
        app
    }

    /// Build a window rooted on `page`.
    ///
    /// When `follows_work_tabs` is false the window is a satellite tool: it keeps
    /// `page`, skips the work-tab strip and dock, and ignores session tab focus
    /// so it feels like a standalone app beside the main shell.
    pub fn open(
        page: Page,
        follows_work_tabs: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let store = Bridge::state(cx);
        let bridge = store.read(cx).clone();
        let editors = ToolEditors::new(window, cx);

        if follows_work_tabs || matches!(page, Page::Coder) {
            Self::apply_coder_editor_settings_to(
                &editors.coder_editors,
                &bridge.coder_settings,
                window,
                cx,
            );
            Bridge::coder_lsp_bind_editors(&editors.coder_editors, cx);
            for (i, editor) in editors.coder_editors.iter().enumerate() {
                Bridge::coder_lsp_subscribe_editor(editor, i as u8, cx);
            }
        }

        cx.subscribe(&editors.launcher_search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();

        cx.subscribe(
            &editors.sc_pr_search,
            |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = input.read(cx).value().to_string();
                    Bridge::git_pulls_set_query(&query, cx);
                    this.bridge = Bridge::state(cx).read(cx).clone();
                    cx.notify();
                }
            },
        )
        .detach();

        // Env picker Confirm → bridge active context (vars sync via forge cursor).
        cx.subscribe(
            &editors.repeater_env,
            |this, _, event: &SelectEvent<Vec<SelectOption<String>>>, cx| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                let id = if value.is_empty() || value.as_str() == REPEATER_ENV_NONE {
                    None
                } else {
                    Some(value.clone())
                };
                Bridge::set_active_context(id, cx);
                this.bridge = Bridge::state(cx).read(cx).clone();
                cx.notify();
            },
        )
        .detach();

        // Collection switcher Confirm → bridge active collection.
        cx.subscribe(
            &editors.repeater_collection,
            |this, _, event: &SelectEvent<Vec<SelectOption<String>>>, cx| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                Bridge::set_active_collection(Some(value.clone()), cx);
                this.bridge = Bridge::state(cx).read(cx).clone();
                cx.notify();
            },
        )
        .detach();

        // Typed `{name}` / `{{name}}` becomes a chip. `set_value` does not emit
        // Change, so loads go through `VariableInput::set_value` instead.
        cx.subscribe_in(
            &editors.repeater_url,
            window,
            |_, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    input.update(cx, |state, cx| {
                        VariableInput::promote_tokens(state, window, cx);
                    });
                }
            },
        )
        .detach();

        // Comparer A/B editors: push into the bridge on every keystroke so the
        // diff table updates live. Typing alone only notifies the editor
        // entity — without these subscriptions `sync_comparer_editors` never
        // runs until some other App re-render.
        for editor in [&editors.cmp_a, &editors.cmp_b] {
            let editor_state = editor.read(cx).editor_state();
            cx.subscribe(&editor_state, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.sync_comparer_editors(cx);
                }
            })
            .detach();
        }

        // Enter in the HTTP history search field applies the filter — the
        // toolbar's search button is icon-only, so Enter is the labeled path.
        cx.subscribe_in(
            &editors.http_search,
            window,
            |_, input, event: &InputEvent, _window, cx| {
                if let InputEvent::PressEnter {
                    secondary: false,
                    shift: false,
                } = event
                {
                    let q = input.read(cx).value().to_string();
                    let _ = Bridge::set_history_search(q, cx);
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &editors.halo_agent_input,
            window,
            |this, input, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Change => {
                        let text = input.read(cx).value().to_string();
                        let cursor = input.read(cx).cursor();
                        let _ = Bridge::halo_agent_completion_sync(&text, cursor, cx);
                        cx.notify();
                    }
                    InputEvent::PressEnter {
                        secondary: false,
                        shift: false,
                    } => {
                        // Enter accepts an open `@` / `/` completion before send.
                        let completion_open = Bridge::state(cx)
                            .read(cx)
                            .coder
                            .agent_completion
                            .as_ref()
                            .is_some_and(|c| !c.items.is_empty());
                        if completion_open
                            && crate::pages::agent::accept_completion(
                                &this.editors.halo_agent_input,
                                window,
                                cx,
                            )
                        {
                            cx.notify();
                            return;
                        }

                        let text = input.read(cx).value().to_string();
                        let running = Bridge::state(cx).read(cx).coder.agent_running;
                        if running && text.trim().is_empty() {
                            let _ = Bridge::halo_agent_interrupt(cx);
                            cx.notify();
                            return;
                        }
                        let _ = Bridge::halo_agent_completion_dismiss(cx);
                        if Bridge::halo_agent_submit(&text, cx).is_ok() {
                            this.editors.halo_agent_input.update(cx, |input, cx| {
                                input.set_value("", window, cx);
                            });
                        }
                        cx.notify();
                    }
                    _ => {}
                }
            },
        )
        .detach();

        // Typing in the harness/model filter narrows the picker's rows.
        cx.subscribe(
            &editors.halo_agent_model_search,
            |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        )
        .detach();

        // Enter in the Source Control "new branch" field creates the branch.
        cx.subscribe_in(
            &editors.sc_branch_name,
            window,
            |_, input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter {
                    secondary: false,
                    shift: false,
                } = event
                {
                    let name = input.read(cx).value().to_string();
                    if name.trim().is_empty() {
                        return;
                    }
                    if Bridge::git_create_branch(&name, cx).is_ok() {
                        input.update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    cx.notify();
                }
            },
        )
        .detach();

        // Coder explorer inline create/rename: Enter commits, Blur commits or cancels.
        cx.subscribe_in(
            &editors.coder_explorer_inline_name,
            window,
            |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter {
                    secondary: false,
                    shift: false,
                } => {
                    let active = Bridge::state(cx).read(cx).coder.explorer_inline.is_some();
                    if !active {
                        return;
                    }
                    let name = input.read(cx).value().to_string();
                    // Keep the field open until a real name is typed (VS Code).
                    if name.trim().is_empty() {
                        return;
                    }
                    let editors = this.editors.coder_editors.clone();
                    let _ = Bridge::coder_explorer_inline_commit(&name, &editors, window, cx);
                    cx.notify();
                }
                InputEvent::Blur => {
                    let active = Bridge::state(cx).read(cx).coder.explorer_inline.is_some();
                    if !active {
                        return;
                    }
                    let name = input.read(cx).value().to_string();
                    if name.trim().is_empty() {
                        let _ = Bridge::coder_explorer_inline_cancel(cx);
                    } else {
                        let editors = this.editors.coder_editors.clone();
                        let _ = Bridge::coder_explorer_inline_commit(&name, &editors, window, cx);
                    }
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        // Typing in the Coder Quick Open search box filters files; Enter opens selected.
        cx.subscribe_in(
            &editors.coder_quick_open_input,
            window,
            |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let query = input.read(cx).value().to_string();
                    let _ = Bridge::coder_quick_open_query(&query, cx);
                    this.bridge = Bridge::state(cx).read(cx).clone();
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.coder_quick_open_confirm(window, cx);
                }
                _ => {}
            },
        )
        .detach();

        let observed = store.clone();
        cx.observe(store.entity(), move |this, _, cx| {
            let next_state = observed.read(cx);
            // Token-only batches leave paint_epoch unchanged, so the shell
            // keeps its snapshot. A toggle or any other data write bumps it.
            if next_state.paint_epoch == this.bridge.paint_epoch {
                return;
            }
            let next = next_state.clone();
            let projects_changed = this.bridge.coder_projects != next.coder_projects;
            let root_changed = this.bridge.coder.root_path != next.coder.root_path;
            this.bridge = next;
            if (projects_changed || root_changed) && this.page == Page::Coder {
                this.sync_app_menu(cx);
            }
            cx.notify();
        })
        .detach();

        let (clock_time, clock_date) = local_clock();
        let last_toasted_message = bridge.last_message.clone();
        let last_toasted_error = bridge.last_error.clone();
        let focus_handle = cx.focus_handle();
        // Without focusing the shell, global chords (including the debugger
        // toggle) only fire when a descendant happens to forward them.
        focus_handle.focus(window, cx);
        let initial_editor_settings =
            matches!(page, Page::Coder).then(|| bridge.coder_settings.clone());
        let mut app = Self {
            focus_handle,
            page,
            follows_work_tabs,
            launcher_open: false,
            notifications_open: false,
            notifications_seen: bridge.debug_logs.len(),
            widget_order: DesktopWidget::ALL.to_vec(),
            dragging_widget: None,
            widget_drop_target: None,
            clock_time,
            clock_date,
            bridge,
            editors,
            last_response_text: None,
            last_history_detail_id: None,
            last_forge_endpoint_id: None,
            last_forge_context_id: None,
            last_forge_env_fingerprint: None,
            last_forge_collection_fingerprint: None,
            last_intercept_paused_id: None,
            last_mock_route_id: None,
            last_chaos_route: None,
            last_jwt_token: String::new(),
            last_comparer_a: String::new(),
            last_comparer_b: String::new(),
            last_fe_search: String::new(),
            last_wl_search: String::new(),
            last_r2_search: String::new(),
            r2_form_seeded: false,
            last_toasted_message,
            last_toasted_error,
            editor_drafts: HashMap::new(),
            last_work_tab_id: None,
            last_work_kind: None,
            last_coder_root: None,
            last_coder_editor_settings: initial_editor_settings,
            last_notes_active_id: None,
            debugger: DebuggerState::default(),
            mockup: follows_work_tabs.then(Mockup::from_env).flatten(),
            icons_warmed: false,
        };
        app.spawn_clock(cx);
        if page == Page::Settings {
            let _ = Bridge::refresh_storage(cx);
        }
        if page == Page::Notes && !app.bridge.notes_loaded {
            let _ = Bridge::refresh_notes(cx);
            app.bridge = Bridge::state(cx).read(cx).clone();
        }
        // Main shell: MITM proxy on by default so crawler / intercept traffic
        // is captured in HTTP History without a manual Start click.
        if follows_work_tabs {
            let (running, busy) = {
                let s = Bridge::state(cx).read(cx);
                (s.proxy.running, s.proxy_busy)
            };
            if !running && !busy {
                components::proxy_button::start_proxy(cx);
                app.bridge = Bridge::state(cx).read(cx).clone();
            }
        }
        // Satellite windows stay off the shared work-tab strip so they feel like
        // standalone tools and do not steal focus from the main shell.
        // Window may open already on Coder (e.g. restore / new-window tool).
        app.sync_app_menu(cx);
        app
    }

    /// Page of this window when it is a tool sub window, not the desktop shell.
    pub(crate) fn sub_window_page(&self) -> Option<Page> {
        if self.follows_work_tabs {
            None
        } else {
            Some(self.page)
        }
    }

    /// Open `page` in a new OS window.
    pub(crate) fn open_in_new_window(page: Page, cx: &mut App) {
        windows::open_page_window(page, cx);
    }

    /// Open `page` in a sub window, or focus the one already showing it.
    pub(crate) fn open_sub_window(page: Page, cx: &mut App) {
        windows::open_sub_window(page, cx);
    }

    fn spawn_clock(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let (time, date) = local_clock();
                let Ok(()) = this.update(cx, |app, cx| {
                    if app.clock_time != time || app.clock_date != date {
                        app.clock_time = time;
                        app.clock_date = date;
                        cx.notify();
                    }
                }) else {
                    break;
                };
            }
        })
        .detach();
    }

    pub(crate) fn reorder_widget(
        &mut self,
        from: DesktopWidget,
        to: DesktopWidget,
        cx: &mut Context<Self>,
    ) {
        if from == to {
            self.clear_widget_drag(cx);
            return;
        }
        let Some(from_ix) = self.widget_order.iter().position(|&w| w == from) else {
            self.clear_widget_drag(cx);
            return;
        };
        let Some(to_ix) = self.widget_order.iter().position(|&w| w == to) else {
            self.clear_widget_drag(cx);
            return;
        };
        let item = self.widget_order.remove(from_ix);
        self.widget_order.insert(to_ix, item);
        self.clear_widget_drag(cx);
    }

    pub(crate) fn set_dragging_widget(
        &mut self,
        widget: Option<DesktopWidget>,
        cx: &mut Context<Self>,
    ) {
        if self.dragging_widget == widget {
            return;
        }
        self.dragging_widget = widget;
        if widget.is_none() {
            self.widget_drop_target = None;
        }
        cx.notify();
    }

    pub(crate) fn set_widget_drop_target(
        &mut self,
        widget: Option<DesktopWidget>,
        cx: &mut Context<Self>,
    ) {
        if self.widget_drop_target == widget {
            return;
        }
        self.widget_drop_target = widget;
        cx.notify();
    }

    pub(crate) fn reset_widget_order(&mut self, cx: &mut Context<Self>) {
        self.widget_order = DesktopWidget::ALL.to_vec();
        self.clear_widget_drag(cx);
    }

    fn clear_widget_drag(&mut self, cx: &mut Context<Self>) {
        self.dragging_widget = None;
        self.widget_drop_target = None;
        cx.notify();
    }
}
