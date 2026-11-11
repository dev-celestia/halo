//! Navigation, app menu, coder project open/close, toasts, and notifications.

use super::*;

impl HaloApp {
    fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        self.launcher_open = false;
        self.notifications_open = false;
        if page == Page::Settings {
            let _ = Bridge::refresh_storage(cx);
        }
        if page == Page::Notes && !self.bridge.notes_loaded {
            let _ = Bridge::refresh_notes(cx);
            self.bridge = Bridge::state(cx).read(cx).clone();
        }
        if page == Page::FileExplorer {
            let _ = Bridge::file_explorer_ensure_root(cx);
            let _ = Bridge::file_explorer_refresh(cx);
            self.bridge = Bridge::state(cx).read(cx).clone();
        }
        if page == Page::Wordlists {
            let _ = Bridge::wordlists_ensure_dir(cx);
            let _ = Bridge::wordlists_sync(cx);
            let _ = Bridge::wordlists_fetch_manifest(false, cx);
            self.bridge = Bridge::state(cx).read(cx).clone();
        }
        if page == Page::R2Storage && !self.bridge.r2.credentials_loaded {
            Bridge::r2_refresh_credentials(cx);
        }
        self.sync_app_menu(cx);
        cx.notify();
    }

    /// Rebuild the native File menu for the focused page (Coder extras only
    /// while [`Page::Coder`] is in front).
    pub(super) fn sync_app_menu(&self, cx: &App) {
        let recent = if self.page == Page::Coder {
            self.bridge.coder_projects.as_slice()
        } else {
            &[]
        };
        let has_open_project = self.page == Page::Coder && self.bridge.coder.root_path.is_some();
        app_menu::install(self.page, recent, has_open_project, cx);
    }

    /// Open folder picker for Coder; focus the Coder tab first when needed.
    pub(super) fn open_coder_folder(&mut self, cx: &mut Context<Self>) {
        if self.page != Page::Coder {
            self.open_page(Page::Coder, cx);
        }
        let _ = Bridge::coder_open_folder(cx);
        self.after_coder_project_opened(cx);
    }

    pub(super) fn open_coder_recent(&mut self, path: &str, cx: &mut Context<Self>) {
        if path.is_empty() {
            return;
        }
        if self.page != Page::Coder {
            self.open_page(Page::Coder, cx);
        }
        if Bridge::coder_open_project(path, cx).is_err() {
            return;
        }
        self.after_coder_project_opened(cx);
    }

    pub(super) fn close_coder_project(&mut self, cx: &mut Context<Self>) {
        if self.page != Page::Coder {
            self.open_page(Page::Coder, cx);
        }
        if Bridge::coder_close_project(cx).is_err() {
            return;
        }
        self.after_coder_project_closed(cx);
    }

    pub(crate) fn open_page(&mut self, page: Page, cx: &mut Context<Self>) {
        if let Some(kind) = page.work_kind() {
            let _ = Bridge::focus_or_open_work_tab(kind, cx);
            self.bridge = Bridge::state(cx).read(cx).clone();
        }
        self.go(page, cx);
    }

    /// Open the tool for the active work tab, or the desktop when the session has none.
    pub(crate) fn follow_active_work(&mut self, cx: &mut Context<Self>) {
        self.bridge = Bridge::state(cx).read(cx).clone();
        if !self.follows_work_tabs {
            cx.notify();
            return;
        }
        match self.bridge.active_work_kind() {
            Some(kind) => self.go(Page::from_work_kind(kind), cx),
            None => self.go(Page::Desktop, cx),
        }
    }

    /// Refresh shell state after Bridge has already opened a Coder project.
    ///
    /// Popup-menu clicks call [`Bridge::coder_open_project`] first (they may not
    /// have a window bound to this entity). Clearing `last_coder_root` makes the
    /// next [`Self::render`] call hydrate editors via `sync_coder_restored_editors`.
    pub(crate) fn after_coder_project_opened(&mut self, cx: &mut Context<Self>) {
        self.bridge = Bridge::state(cx).read(cx).clone();
        self.last_coder_root = None;
        if self.follows_work_tabs {
            self.page = Page::Coder;
        }
        self.sync_app_menu(cx);
        cx.notify();
    }

    /// Refresh shell state after Bridge has closed the active Coder project.
    ///
    /// Leaves [`Self::last_coder_root`] alone so the next render sees
    /// `root_path: None` ≠ previous root and clears editor widgets via
    /// [`Self::sync_coder_restored_editors`].
    pub(crate) fn after_coder_project_closed(&mut self, cx: &mut Context<Self>) {
        self.bridge = Bridge::state(cx).read(cx).clone();
        if self.follows_work_tabs {
            self.page = Page::Coder;
        }
        self.sync_app_menu(cx);
        cx.notify();
    }

    /// Always create a new work tab for `page` (the tab-strip + menu).
    pub(crate) fn open_new_tab(&mut self, page: Page, cx: &mut Context<Self>) {
        let Some(kind) = page.work_kind() else {
            self.go(Page::Desktop, cx);
            return;
        };
        let _ = Bridge::open_work_tab(kind, cx);
        self.bridge = Bridge::state(cx).read(cx).clone();
        self.go(page, cx);
    }

    pub(crate) fn toggle_launcher(&mut self, cx: &mut Context<Self>) {
        self.launcher_open = !self.launcher_open;
        if self.launcher_open {
            self.notifications_open = false;
        }
        cx.notify();
    }

    pub(crate) fn close_launcher(&mut self, cx: &mut Context<Self>) {
        if !self.launcher_open {
            return;
        }
        self.launcher_open = false;
        cx.notify();
    }

    pub(crate) fn toggle_notifications(&mut self, cx: &mut Context<Self>) {
        self.notifications_open = !self.notifications_open;
        if self.notifications_open {
            self.launcher_open = false;
            self.mark_notifications_seen(cx);
            return;
        }
        cx.notify();
    }

    pub(crate) fn close_notifications(&mut self, cx: &mut Context<Self>) {
        if !self.notifications_open {
            return;
        }
        self.notifications_open = false;
        cx.notify();
    }

    pub(crate) fn mark_notifications_seen(&mut self, cx: &mut Context<Self>) {
        self.notifications_seen = self.bridge.debug_logs.len();
        cx.notify();
    }

    pub(super) fn notification_unread(&self) -> usize {
        self.bridge
            .debug_logs
            .len()
            .saturating_sub(self.notifications_seen)
    }

    /// Push toasts for new Bridge `last_message` / `last_error` values.
    ///
    /// Skips progress placeholders ending in `…` / `...`. When both fields change
    /// and the new error is set, only the error toast is shown.
    pub(super) fn flush_bridge_toasts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.follows_work_tabs {
            return;
        }
        let err_changed = self.bridge.last_error != self.last_toasted_error;
        let msg_changed = self.bridge.last_message != self.last_toasted_message;

        if !err_changed && !msg_changed {
            return;
        }

        if err_changed {
            self.last_toasted_error = self.bridge.last_error.clone();
            if let Some(err) = self.bridge.last_error.clone() {
                if msg_changed {
                    self.last_toasted_message = self.bridge.last_message.clone();
                }
                if self.bridge.last_error_info {
                    // Git failures already wrote their raw output to the Git
                    // Output dock; the toast only says where to look.
                    crate::toast::close_error(window, cx);
                    crate::toast::info(window, cx, err);
                } else if let Some(server_id) = self.bridge.coder_lsp_offer_install.clone() {
                    let label = self
                        .bridge
                        .coder_lsp_offer_install_label
                        .clone()
                        .unwrap_or_else(|| "Install".into());
                    crate::toast::error_with_install(window, cx, err, server_id, label);
                } else {
                    crate::toast::error(window, cx, err);
                }
                return;
            } else {
                crate::toast::close_error(window, cx);
                crate::toast::close_info(window, cx);
            }
        }

        if msg_changed {
            self.last_toasted_message = self.bridge.last_message.clone();
            if let Some(msg) = self.bridge.last_message.clone() {
                if !crate::toast::is_progress_message(&msg)
                    && !crate::toast::is_status_bar_message(&msg)
                {
                    crate::toast::success(window, cx, msg);
                }
            } else {
                crate::toast::close_success(window, cx);
            }
        }
    }

    pub(super) fn align_page_to_tab(&mut self, cx: &mut Context<Self>) {
        if !self.follows_work_tabs || self.page == Page::Desktop {
            return;
        }
        match self.bridge.active_work_kind() {
            Some(kind) => {
                let page = Page::from_work_kind(kind);
                if self.page != page {
                    self.page = page;
                    if page == Page::Settings {
                        let _ = Bridge::refresh_storage(cx);
                    }
                }
            }
            None => self.page = Page::Desktop,
        }
    }
}
