//! Focusable impl, DevTools toggle, the root renderer, and small free helpers.

use super::*;

impl Focusable for HaloApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl HaloApp {
    /// Toggle the bottom DevTools panel (shortcut, tray, or menu).
    pub(crate) fn toggle_debugger(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.debugger.toggle();
        if self.debugger.open {
            Bridge::push_debug_log(
                halo_bridge::DebugLogLevel::Info,
                "ui",
                "Debugger opened",
                cx,
            );
            let _ = Bridge::refresh_storage(cx);
            self.bridge = Bridge::state(cx).read(cx).clone();
            toast::info(
                window,
                cx,
                "Debugger open — Elements · Network · Storage · Logs",
            );
        } else {
            self.debugger.inspect_mode = false;
            toast::info(window, cx, "Debugger closed");
        }
        cx.notify();
    }
}

impl Render for HaloApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Mockup surface replaces the whole shell: the app's chrome is skipped
        // and AppKit's own lights are pushed off-frame, so the review shows
        // exactly one traffic-light cluster — the drawn one.
        if let Some(mockup) = self.mockup {
            return os_shell::render_mockup(mockup, window, cx);
        }

        self.apply_pending_transfers(window, cx);
        self.align_page_to_tab(cx);
        self.sync_work_tab_editors(window, cx);
        self.sync_notes_active_editor(window, cx);
        self.sync_coder_restored_editors(window, cx);
        self.sync_coder_editor_settings(window, cx);
        self.flush_bridge_toasts(window, cx);
        self.sync_response_editor(window, cx);
        self.sync_forge_editors(window, cx);
        self.sync_history_editors(window, cx);
        self.sync_intercept_editor(window, cx);
        self.sync_mock_editors(window, cx);
        self.sync_mock_chaos(window, cx);
        self.sync_jwt_decode(cx);
        self.sync_comparer_editors(cx);
        self.sync_explorer_searches(cx);
        self.sync_terminal_surfaces();
        self.seed_r2_form(window, cx);

        if !cx.has_active_drag()
            && (self.dragging_widget.is_some() || self.widget_drop_target.is_some())
        {
            self.dragging_widget = None;
            self.widget_drop_target = None;
        }

        let weak = cx.weak_entity();
        clear_regions();
        register_workspace_regions(self.page, &self.bridge);
        // First launch only: Doctor probes are otherwise refreshed when that
        // page is the restored tab, so the setup dialog would open with no rows.
        // `checking` flips true inside the refresh, which keeps later paints
        // from starting another scan.
        if self.follows_work_tabs
            && !self.bridge.onboarding_completed
            && self.bridge.doctor.items.is_empty()
            && !self.bridge.doctor.checking
        {
            Bridge::doctor_refresh(cx);
            self.bridge = Bridge::state(cx).read(cx).clone();
        }

        // Display scale for this window: marks are rasterized 1:1 with the
        // device grid, so a 1x display needs the 1x raster and a 2x display
        // the 2x one.
        let scale = window.scale_factor();
        if !self.icons_warmed {
            self.icons_warmed = true;
            app_icons::warm_up(scale);
        }
        let workspace = if self.page == Page::Desktop {
            os_shell::render_desktop(
                &self.bridge,
                &self.widget_order,
                self.dragging_widget,
                self.widget_drop_target,
                &weak,
                scale,
                cx,
            )
            .into_any_element()
        } else {
            os_shell::render_app_window(self.page, &self.bridge, &self.editors, &weak, window, cx)
                .into_any_element()
        };

        let title = match self.page {
            Page::Desktop => "Halo".to_string(),
            other => format!("Halo — {}", other.label()),
        };

        div()
            .id("halo")
            .role(Role::Group)
            .aria_label("Halo")
            .key_context("HaloApp")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|_, _: &ToggleTheme, _, cx| Self::toggle_theme(cx)))
            .on_action(cx.listener(|_, _: &ToggleProxy, _, cx| Self::toggle_proxy(cx)))
            .on_action(cx.listener(|this, _: &SaveCoderFile, window, cx| {
                this.save_coder_file(window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewCoderProject, _, cx| {
                // Same as Open Folder until a project scaffolder exists.
                this.open_coder_folder(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenCoderFolder, _, cx| {
                this.open_coder_folder(cx);
            }))
            .on_action(cx.listener(|this, action: &OpenCoderRecent, _, cx| {
                this.open_coder_recent(&action.path, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseCoderProject, _, cx| {
                this.close_coder_project(cx);
            }))
            .on_action(|_: &OpenCoderSettings, _, cx| {
                // Same as the Coder status-bar gear — satellite settings window,
                // landed on the editor section rather than General.
                windows::open_settings_window(SettingsTab::CoderEditor, cx);
            })
            .on_action(cx.listener(|this, _: &ToggleCoderTerminal, window, cx| {
                let _ = Bridge::coder_toggle_terminal(cx);
                this.bridge = Bridge::state(cx).read(cx).clone();
                if this.bridge.coder.terminal_open {
                    if let Some(id) = this.bridge.coder.active_terminal_id.clone() {
                        this.editors
                            .terminal_surfaces
                            .get_or_create(&id, cx)
                            .update(cx, |surface, cx| {
                                surface.focus(window, cx);
                            });
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleCoderWordWrap, window, cx| {
                this.toggle_coder_word_wrap(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &TriggerCoderCompletions, window, cx| {
                    // Global chord, so gate it on the Coder page actually being in
                    // front: elsewhere the focused editor is not a Coder pane and
                    // the bridge would address whatever pane was focused last.
                    if this.page != Page::Coder {
                        return;
                    }
                    Bridge::coder_show_completions(window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &CommitSourceControl, window, cx| {
                // ⌘/Ctrl+Enter is global, so gate it on the Source Control
                // panel actually being open with something staged.
                let (panel_open, has_staged) = {
                    let state = Bridge::state(cx).read(cx);
                    (
                        state.active_work_kind() == Some(WorkKind::SourceControl),
                        !state.git.staged.is_empty(),
                    )
                };
                if !panel_open || !has_staged {
                    return;
                }
                let msg = this.editors.sc_commit_msg.read(cx).value().to_string();
                if msg.trim().is_empty() {
                    this.editors
                        .sc_commit_msg
                        .update(cx, |input, cx| input.focus(window, cx));
                    return;
                }
                if Bridge::git_commit(&msg, cx).is_ok() {
                    this.editors.sc_commit_msg.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                    });
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleDebugger, window, cx| {
                this.toggle_debugger(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleCoderQuickOpen, window, cx| {
                this.toggle_coder_quick_open(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CoderQuickOpenUp, _, cx| {
                this.coder_quick_open_select(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &CoderQuickOpenDown, _, cx| {
                this.coder_quick_open_select(1, cx);
            }))
            .on_action(cx.listener(|this, _: &CoderQuickOpenClose, window, cx| {
                this.close_coder_quick_open(window, cx);
            }))
            .on_action(|_: &NewWindow, _, cx| {
                // File → New Window / ⌘N: fresh desktop shell. Context menus
                // that open a specific tool still call `open_in_new_window`.
                Self::open_in_new_window(Page::Desktop, cx);
            })
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .on_action(|_: &MinimizeWindow, window, _| window.minimize_window())
            .on_action(|_: &ZoomWindow, window, _| window.zoom_window())
            .on_action(|_: &ToggleFullScreen, window, _| window.toggle_fullscreen())
            .on_action(|_: &QuitApp, _, cx| cx.quit())
            .on_action(|_: &HideApp, _, cx| cx.hide())
            .on_action(|_: &HideOtherApps, _, cx| cx.hide_other_apps())
            .on_action(|_: &ShowAllApps, _, cx| cx.unhide_other_apps())
            .on_action(|_: &ShowHelp, window, cx| {
                toast::info(
                    window,
                    cx,
                    "Help — File → New Window, Window → Minimize / Zoom, View → Full Screen.",
                );
            })
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                app_window::Window::new("halo", title)
                    .chrome(WindowChrome::Native)
                    .when(self.follows_work_tabs, |this| {
                        this.title_bar(os_shell::render_work_tabs(
                            &self.bridge,
                            self.page,
                            &weak,
                            scale,
                            cx,
                        ))
                    })
                    .child(
                        div()
                            .id("workspace")
                            .relative()
                            .flex_1()
                            .w_full()
                            .h_full()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .child(
                                div()
                                    .relative()
                                    .flex_1()
                                    .w_full()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .child(workspace)
                                    .child(region_bounds_sensor(
                                        "workspace",
                                        "Workspace",
                                        DebugRegionMeta {
                                            padding: None,
                                            gap: None,
                                            notes: Some(format!("active_page={}", self.page.id())),
                                        },
                                    ))
                                    .when(self.follows_work_tabs && self.launcher_open, |this| {
                                        this.child(os_shell::render_launcher(
                                            &self.bridge,
                                            &self.editors.launcher_search,
                                            &weak,
                                            scale,
                                            window,
                                            cx,
                                        ))
                                    })
                                    .when(
                                        self.follows_work_tabs && self.notifications_open,
                                        |this| {
                                            this.child(os_shell::render_notifications(
                                                &self.bridge,
                                                &weak,
                                                cx,
                                            ))
                                        },
                                    )
                                    .when(
                                        self.follows_work_tabs && !self.bridge.onboarding_completed,
                                        |this| {
                                            this.child(os_shell::render_onboarding_dialog(
                                                &self.bridge,
                                                cx,
                                            ))
                                        },
                                    )
                                    .when(self.debugger.inspect_mode, |this| {
                                        let weak = weak.clone();
                                        this.child(render_inspect_overlay(weak))
                                    })
                                    .when(theme::is_switching(cx), |this| {
                                        this.child(theme::render_switching_overlay(cx))
                                    }),
                            )
                            .when(self.debugger.open, |this| {
                                let weak = weak.clone();
                                this.child(render_debugger(
                                    &self.debugger,
                                    &self.bridge,
                                    {
                                        let weak = weak.clone();
                                        move |cx| {
                                            let _ = weak.update(cx, |app, cx| {
                                                app.debugger.open = false;
                                                app.debugger.inspect_mode = false;
                                                cx.notify();
                                            });
                                        }
                                    },
                                    {
                                        let weak = weak.clone();
                                        move |tab, cx| {
                                            let _ = weak.update(cx, |app, cx| {
                                                app.debugger.set_tab(tab);
                                                cx.notify();
                                            });
                                        }
                                    },
                                    {
                                        let weak = weak.clone();
                                        move |cx| {
                                            let _ = weak.update(cx, |app, cx| {
                                                app.debugger.inspect_mode =
                                                    !app.debugger.inspect_mode;
                                                cx.notify();
                                            });
                                        }
                                    },
                                    {
                                        let weak = weak.clone();
                                        move |id, cx| {
                                            set_selected(Some(id));
                                            let _ = weak.update(cx, |app, cx| {
                                                app.debugger.tab = DebuggerTab::Elements;
                                                app.debugger.inspect_mode = false;
                                                cx.notify();
                                            });
                                        }
                                    },
                                    {
                                        let weak = weak.clone();
                                        move |id, cx| {
                                            let _ = weak.update(cx, |app, cx| {
                                                app.debugger.selected_network_id = id;
                                                cx.notify();
                                            });
                                        }
                                    },
                                    {
                                        let weak = weak.clone();
                                        move |path, cx| {
                                            let _ = weak.update(cx, |app, cx| {
                                                app.debugger.selected_prefs_path = path;
                                                cx.notify();
                                            });
                                        }
                                    },
                                    cx,
                                ))
                            })
                            .when(self.follows_work_tabs, |this| {
                                this.child(os_shell::render_dock(
                                    self.page,
                                    self.launcher_open,
                                    self.notifications_open,
                                    self.notification_unread(),
                                    self.debugger.open,
                                    self.clock_time.clone(),
                                    self.clock_date.clone(),
                                    &self.bridge,
                                    &weak,
                                    scale,
                                    cx,
                                ))
                            }),
                    ),
            )
            .into_any_element()
    }
}

pub(super) fn local_clock() -> (SharedString, SharedString) {
    let now = Local::now();
    (
        now.format("%H:%M").to_string().into(),
        now.format("%a %b %d").to_string().into(),
    )
}

fn register_workspace_regions(page: Page, bridge: &BridgeState) {
    // Seed ids before the debugger body reads `region_list` this frame.
    // Matching `region_bounds_sensor`s below refresh window bounds during prepaint.
    register_region(
        "workspace",
        "Workspace",
        None,
        DebugRegionMeta {
            padding: None,
            gap: None,
            notes: Some(format!("active_page={}", page.id())),
        },
    );
    register_region(
        "dock",
        "Dock",
        None,
        DebugRegionMeta {
            padding: Some("dock chrome".into()),
            gap: None,
            notes: Some(format!(
                "proxy={} tabs={}",
                if bridge.proxy.running { "on" } else { "off" },
                bridge.work_tabs.len()
            )),
        },
    );
}

fn render_inspect_overlay(weak: WeakEntity<HaloApp>) -> impl IntoElement {
    div()
        .id("debugger-inspect-overlay")
        .absolute()
        .inset_0()
        .cursor_crosshair()
        .on_mouse_down(MouseButton::Left, move |ev, _, cx| {
            let x = f32::from(ev.position.x);
            let y = f32::from(ev.position.y);
            if select_region_at(x, y).is_some() {
                let _ = weak.update(cx, |app, cx| {
                    app.debugger.tab = DebuggerTab::Elements;
                    app.debugger.inspect_mode = false;
                    cx.notify();
                });
            }
            cx.stop_propagation();
        })
}
