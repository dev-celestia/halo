//! Window-local, byte-aware retention of explicit TextViewState entities.
use std::collections::HashMap;

use gpui::*;
use gpui_component::text::{MarkdownExtensions, SelectionFormat, TextViewState};

use super::{
    markdown,
    transcript_state::{CACHE_BYTES, CACHE_ENTRIES, ChatKey, text_cost},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PartKey {
    pub chat: ChatKey,
    pub message: String,
    pub part: usize,
}

pub struct CachedText {
    pub source: SharedString,
    pub state: Entity<TextViewState>,
    pub extensions: MarkdownExtensions,
    pub used: u64,
    pub cost: usize,
    pub _subscription: Option<Subscription>,
}

#[derive(Default)]
pub struct TextCache {
    pub entries: HashMap<PartKey, CachedText>,
    pub created: usize,
    pub updated: usize,
    pub evicted: usize,
    clock: u64,
}

impl TextCache {
    pub fn get(
        &mut self,
        key: PartKey,
        source: &str,
        cx: &mut App,
    ) -> (Entity<TextViewState>, MarkdownExtensions, bool) {
        self.clock += 1;
        let mut created = false;
        let entry = self.entries.entry(key.clone()).or_insert_with(|| {
            self.created += 1;
            created = true;
            CachedText {
                source: source.to_owned().into(),
                state: cx.new(|cx| {
                    TextViewState::markdown(source, cx)
                        .selectable(true)
                        .selection_format(SelectionFormat::Source)
                }),
                extensions: markdown::extensions(key.chat.part_id(&key.message, key.part).into()),
                used: 0,
                cost: text_cost(source.len()),
                _subscription: None,
            }
        });
        if entry.source.as_ref() != source {
            // set_text detects append vs replacement, including same-length
            // authoritative corrections; it keeps the unchanged parsed prefix.
            entry
                .state
                .update(cx, |state, cx| state.set_text(source, cx));
            entry.source = source.to_owned().into();
            entry.cost = text_cost(source.len());
            self.updated += 1;
        }
        entry.used = self.clock;
        (entry.state.clone(), entry.extensions.clone(), created)
    }

    pub fn bytes(&self) -> usize {
        self.entries.values().map(|entry| entry.cost).sum()
    }

    pub fn trim(&mut self, active: Option<&PartKey>, cx: &App) {
        let mut bytes = self.bytes();
        while self.entries.len() > CACHE_ENTRIES || bytes > CACHE_BYTES {
            let oldest = self
                .entries
                .iter()
                .filter(|(key, entry)| {
                    Some(*key) != active
                        && !entry.state.read(cx).is_selecting()
                        && entry.state.read(cx).selected_source_range().is_none()
                })
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone());
            let Some(key) = oldest else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                bytes -= entry.cost;
                self.evicted += 1;
            }
        }
    }
}
