//! User actions: theme, proxy, saves, quick open, and coder editor settings.

use super::*;

impl HaloApp {
    pub(super) fn toggle_theme(cx: &mut App) {
        theme::toggle_mode(cx);
    }

    pub(super) fn toggle_proxy(cx: &mut App) {
        components::toggle_proxy(cx);
    }

    pub(super) fn save_coder_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page == Page::Notes {
            self.save_active_note(cx);
            return;
        }
        if self.bridge.coder.focused_active_path().is_none() {
            return;
        }
        let pane = self.bridge.coder.focused_pane;
        let text = self.editors.coder_editor(pane).read(cx).text(cx);
        let _ = Bridge::coder_save_active(&text, window, cx);
        self.bridge = Bridge::state(cx).read(cx).clone();
        cx.notify();
    }

    fn save_active_note(&mut self, cx: &mut Context<Self>) {
        let text = self.editors.notes_editor.read(cx).text(cx);
        if let Some(note) = self.bridge.active_note() {
            let id = note.id.clone();
            let name = note.name.clone();
            let _ = Bridge::save_note(&id, &name, &text, cx);
            self.bridge = Bridge::state(cx).read(cx).clone();
            cx.notify();
        }
    }

    pub(super) fn toggle_coder_quick_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page != Page::Coder {
            self.open_page(Page::Coder, cx);
        }
        if self.bridge.coder.root_path.is_none() {
            self.open_coder_folder(cx);
            return;
        }
        match Bridge::coder_quick_open_toggle(cx) {
            Ok(true) => {
                self.editors.coder_quick_open_input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                    input.focus(window, cx);
                });
            }
            Ok(false) => {}
            Err(e) => {
                crate::toast::error(window, cx, &e.to_string());
            }
        }
        self.bridge = Bridge::state(cx).read(cx).clone();
        cx.notify();
    }

    pub(super) fn close_coder_quick_open(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        Bridge::coder_quick_open_close(cx);
        self.bridge = Bridge::state(cx).read(cx).clone();
        cx.notify();
    }

    pub(super) fn coder_quick_open_select(&mut self, delta: isize, cx: &mut Context<Self>) {
        Bridge::coder_quick_open_select(delta, cx);
        self.bridge = Bridge::state(cx).read(cx).clone();
        cx.notify();
    }

    pub(super) fn coder_quick_open_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = Bridge::coder_quick_open_confirm(&self.editors.coder_editors, window, cx);
        self.bridge = Bridge::state(cx).read(cx).clone();
        cx.notify();
    }

    pub(super) fn sync_notes_active_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current_id = self.bridge.notes_active_id.clone();
        if self.last_notes_active_id == current_id {
            return;
        }
        if let Some(prev_id) = self.last_notes_active_id.take() {
            let text = self.editors.notes_editor.read(cx).text(cx);
            if let Some(prev_note) = self.bridge.notes.iter().find(|n| n.id == prev_id) {
                let _ = Bridge::save_note(&prev_id, &prev_note.name, &text, cx);
            }
        }
        self.last_notes_active_id = current_id.clone();
        if let Some(note) = self.bridge.active_note() {
            let content = note.note.clone();
            let name = note.name.clone();
            self.editors.notes_editor.update(cx, |e, cx| {
                e.set_text(content, window, cx);
            });
            self.editors.notes_rename.update(cx, |s, cx| {
                s.set_value(name, window, cx);
            });
        }
    }

    /// Push IDE editor prefs into every Coder pane buffer.
    pub(super) fn apply_coder_editor_settings_to(
        editors: &[Entity<celestia_ui::components::CodeEditor>],
        settings: &CoderSettings,
        window: &mut Window,
        cx: &mut App,
    ) {
        let font_size = px(f32::from(settings.font_size));
        let tab = TabSize {
            tab_size: usize::from(settings.tab_size).max(1),
            hard_tabs: false,
        };
        for editor in editors {
            editor.update(cx, |editor, cx| {
                editor.set_minimap(settings.minimap, cx);
                editor.set_font_size(font_size, cx);
                match halo_bridge::font_family(&settings.font_id) {
                    Some(family) => editor.set_font_family(family, cx),
                    None => editor.clear_font_family(cx),
                }
            });
            editor.read(cx).editor_state().update(cx, |state, cx| {
                state.set_soft_wrap(settings.word_wrap, window, cx);
                state.set_line_number(settings.line_numbers, window, cx);
                state.set_tab_size(tab, cx);
            });
        }
    }

    pub(super) fn sync_coder_editor_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.page != Page::Coder {
            return;
        }
        let settings = self.bridge.coder_settings.clone();
        if self.last_coder_editor_settings.as_ref() == Some(&settings) {
            return;
        }
        Self::apply_coder_editor_settings_to(&self.editors.coder_editors, &settings, window, cx);
        self.last_coder_editor_settings = Some(settings);
    }

    pub(super) fn toggle_coder_word_wrap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = !self.bridge.coder_settings.word_wrap;
        Bridge::update_coder_settings(cx, move |s| s.word_wrap = next);
        self.bridge = Bridge::state(cx).read(cx).clone();
        self.last_coder_editor_settings = None;
        self.sync_coder_editor_settings(window, cx);
        cx.notify();
    }

    /// After a project open/restore, push persisted tabs into the editor widgets.
    pub(super) fn sync_coder_restored_editors(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.page != Page::Coder {
            return;
        }
        let root = self.bridge.coder.root_path.clone();
        if root == self.last_coder_root {
            return;
        }
        self.last_coder_root = root;
        let _ = Bridge::coder_sync_editors(&self.editors.coder_editors, window, cx);
        self.bridge = Bridge::state(cx).read(cx).clone();
    }
}
