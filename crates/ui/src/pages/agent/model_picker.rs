//! Harness + model dropdown — harness strip, search, grouped model rows.
//!
//! Top strip: Pi / OpenCode / Google (Antigravity) / Cursor + refresh.
//! Search field, then provider-grouped models.
//! Visible models 1–9 show ⌘/Ctrl shortcut pills.

use std::collections::BTreeMap;

use celestia_ui::components::Input;
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::InputState;
use celestia_ui::components::primitive::loader::Loader;
use celestia_ui::components::primitive::textarea::TextareaState;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState};

use crate::actions::PickAgentModel;

/// Single-line row height.
const ROW_HEIGHT: Pixels = px(30.);
/// Leading vendor / harness mark size.
const MARK_SIZE: Pixels = px(16.);
/// Filter-strip icon hit target.
const FILTER_HIT: Pixels = px(28.);
/// Whole sheet height before the model list scrolls — keeps the flyout inside
/// the window when the composer sits low and OpenCode returns a long catalog.
pub(super) const PICKER_MAX: Pixels = px(320.);
/// Scroll viewport for the model rows (sheet chrome sits above this).
const LIST_MAX: Pixels = px(240.);
/// Fixed sheet width — independent of the truncated toolbar chip.
pub(super) const PICKER_WIDTH: Pixels = px(360.);

pub(super) fn render_picker(
    state: &BridgeState,
    search: &Entity<InputState>,
    draft: &Entity<TextareaState>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let border = theme.border;
    let accent = theme.accent;
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;
    let primary = theme.primary;
    let ring = theme.ring;
    let radius = theme.radius;
    // Sheet pad is 4px, so the row corner is the outer radius minus that inset.
    let row_radius = (theme.radius - px(4.)).max(px(0.));
    let is_dark = theme.is_dark();
    // The flyout must read as its own sheet: `popover` is also the prompt card's
    // fill, so reusing it merged the two and let the card's edge show through.
    let surface = elevated_surface(theme.popover, is_dark);

    // Strip shows the four harnesses (Pi / OpenCode / Antigravity / Cursor).
    // Prefer the cached catalog; fall back to static labels so the flyout
    // paints immediately while refresh_catalogs finishes off-thread.
    let harnesses: Vec<(String, String)> = {
        let cached: Vec<(String, String)> = Bridge::halo_agent_list_harnesses(cx)
            .unwrap_or_default()
            .into_iter()
            .filter(|h| matches!(h.id.as_str(), "pi" | "opencode" | "antigravity" | "cursor"))
            .map(|h| (h.id, h.display_name))
            .collect();
        if cached.is_empty() {
            vec![
                ("pi".into(), "Pi".into()),
                ("opencode".into(), "OpenCode".into()),
                ("antigravity".into(), "Antigravity".into()),
                ("cursor".into(), "Cursor".into()),
            ]
        } else {
            cached
        }
    };
    let models: Vec<(String, String)> = state
        .coder
        .agent_model_catalog
        .iter()
        .map(|m| (m.id.clone(), m.display_name.clone()))
        .collect();
    let loading = state.coder.agent_catalog_loading;
    let query = search.read(cx).value().trim().to_lowercase();
    let current_harness = state.coder.agent_harness.clone();
    let current_model = state.coder.agent_model.clone();
    let current_harness_label = harnesses
        .iter()
        .find(|(id, _)| id == &current_harness)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| "model".into());

    let shortcut_ids = shortcut_model_ids(&models, &query);

    let colors = RowColors {
        accent,
        foreground,
        muted,
        row_radius,
    };

    // Size to the rows (up to LIST_MAX), then scroll. Do not use flex_1 +
    // h_full here: with an auto-height sheet that pair resolves to 0px and
    // the catalog disappears under the search field.
    let mut list = div()
        .id("coder-agent-picker-list")
        .w_full()
        .max_h(LIST_MAX)
        .flex()
        .flex_col()
        .p(px(4.))
        .gap(px(1.));

    let mut any_model = false;
    let grouped = group_models_by_provider(&models, &query);
    for (provider, group) in grouped {
        if group.is_empty() {
            continue;
        }
        any_model = true;
        list = list.child(section_label(&provider_label(&provider), muted));
        for (id, name) in group {
            let shortcut = shortcut_for(&shortcut_ids, &id);
            let (mark, tint) = model_mark(&current_harness, &id);
            let selected = current_model.as_deref() == Some(id.as_str());
            let draft = draft.clone();
            list = list.child(
                model_row(
                    RowSpec {
                        id: SharedString::from(format!("coder-agent-model-{id}")),
                        title: name.into(),
                        shortcut,
                        mark,
                        tint,
                        selected,
                        fallback: PhosphorIcon::Cpu,
                    },
                    colors,
                )
                .on_click(move |_, window, cx| {
                    let _ = Bridge::halo_agent_set_model(&id, cx);
                    draft.update(cx, |draft, cx| draft.focus(window, cx));
                }),
            );
        }
    }

    // Nothing to show yet: name what is happening instead of leaving a blank
    // sheet, and keep the skeleton rows so the flyout height does not jump when
    // the catalog lands.
    if loading && !any_model {
        list = list.child(loading_row(&current_harness_label, muted));
        list = list.child(catalog_skeleton(theme.skeleton, row_radius));
    }

    if !any_model && !loading && !query.is_empty() {
        list = list.child(
            div()
                .w_full()
                .px(px(10.))
                .py(px(14.))
                .text_xs()
                .text_color(muted)
                .child("No model matches that search."),
        );
    } else if !any_model && !loading && query.is_empty() {
        list = list.child(
            div()
                .w_full()
                .px(px(10.))
                .py(px(14.))
                .text_xs()
                .text_color(muted)
                .child("No models cached yet. Tap refresh in the strip."),
        );
    }

    // Match Dialog's max_h pattern: the sheet grows with content up to
    // PICKER_MAX, then only the model list scrolls.
    div()
        .id("coder-agent-picker")
        .key_context("AgentModelPicker")
        .relative()
        .w(PICKER_WIDTH)
        .min_w(PICKER_WIDTH)
        .max_h(PICKER_MAX)
        .flex()
        .flex_col()
        .rounded(radius)
        .border_1()
        .border_color(border)
        .bg(surface)
        .overflow_hidden()
        .on_action({
            let search = search.clone();
            let draft = draft.clone();
            move |action: &PickAgentModel, window, cx| {
                let query = search.read(cx).value().trim().to_lowercase();
                let id = {
                    let state = Bridge::state(cx).read(cx);
                    pick_shortcut_from_state(&state, action.index as usize, &query)
                };
                if let Some(id) = id {
                    let _ = Bridge::halo_agent_set_model(&id, cx);
                    draft.update(cx, |draft, cx| draft.focus(window, cx));
                }
            }
        })
        .child(filter_strip(
            &harnesses,
            &current_harness,
            search,
            muted,
            foreground,
            primary,
            accent,
            is_dark,
            loading,
            cx,
        ))
        .child(
            div()
                .id("coder-agent-picker-search")
                .relative()
                .w_full()
                .flex_none()
                .px(px(10.))
                .pt(px(6.))
                .pb(px(8.))
                .bg(surface)
                .child(
                    div()
                        .w_full()
                        .h(px(32.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(row_radius)
                        .border_1()
                        .border_color(ring.opacity(0.85))
                        .bg(if is_dark {
                            theme.muted.opacity(0.35)
                        } else {
                            theme.muted.opacity(0.5)
                        })
                        .child(
                            Phosphor::new(PhosphorIcon::MagnifyingGlass)
                                .size(px(14.))
                                .color(muted),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                // Placeholder text is set on the InputState when the
                                // picker opens / harness changes (`Search {harness} models.`).
                                .child(Input::new(search).appearance(false).cleanable(true)),
                        ),
                ),
        )
        .child(
            div()
                .id("coder-agent-picker-scroll")
                .w_full()
                .flex_none()
                .bg(surface)
                .child(list.overflow_y_scrollbar()),
        )
}

/// One step above `theme.popover`, always fully opaque — the flyout has to hide
/// whatever it covers (the prompt card shares the plain `popover` fill).
pub(super) fn elevated_surface(base: Hsla, is_dark: bool) -> Hsla {
    let delta = if is_dark { 0.035 } else { -0.03 };
    hsla(base.h, base.s, (base.l + delta).clamp(0., 1.), 1.)
}

/// "Loading <Harness> models…" with the shared grid loader, so an empty catalog
/// never reads as "no models" while the probe is still running.
fn loading_row(harness: &str, muted: Hsla) -> impl IntoElement {
    div()
        .id("coder-agent-picker-loading")
        .w_full()
        .flex_none()
        .h(ROW_HEIGHT)
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(8.))
        .aria_label("Loading models")
        .child(Loader::grid_shift().size(px(12.)).color(muted))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(muted)
                .child(format!("Loading {harness} models…")),
        )
}

/// Placeholder list that mirrors section labels + model rows (mark + title).
/// Sized to a typical catalog so the flyout height stays stable while loading.
fn catalog_skeleton(skeleton: Hsla, row_radius: Pixels) -> impl IntoElement {
    // Two provider groups × a few rows ≈ a typical OpenCode/Pi catalog.
    const GROUPS: &[(f32, &[f32])] =
        &[(64., &[168., 132., 152., 120.]), (72., &[144., 176., 128.])];

    let mut block = div()
        .id("coder-agent-picker-skeleton")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(1.))
        .aria_label("Loading models");

    for (group_i, (label_w, rows)) in GROUPS.iter().enumerate() {
        block = block.child(
            div().w_full().px(px(8.)).pt(px(8.)).pb(px(2.)).child(
                div()
                    .h(px(10.))
                    .w(px(*label_w))
                    .rounded_full()
                    .bg(skeleton.opacity(0.85)),
            ),
        );
        for (row_i, title_w) in rows.iter().enumerate() {
            block = block.child(skeleton_model_row(
                SharedString::from(format!("coder-agent-skel-{group_i}-{row_i}")),
                skeleton,
                row_radius,
                px(*title_w),
                row_i % 2 == 0,
            ));
        }
    }

    block
}

fn skeleton_model_row(
    id: SharedString,
    skeleton: Hsla,
    row_radius: Pixels,
    title_w: Pixels,
    with_shortcut: bool,
) -> impl IntoElement {
    div()
        .id(id)
        .w_full()
        .h(ROW_HEIGHT)
        .flex_none()
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(row_radius)
        .child(
            div()
                .size(MARK_SIZE)
                .flex_none()
                .rounded_full()
                .bg(skeleton),
        )
        .child(
            div().flex_1().min_w_0().child(
                div()
                    .h(px(10.))
                    .w(title_w)
                    .max_w_full()
                    .rounded_full()
                    .bg(skeleton),
            ),
        )
        .when(with_shortcut, |el| {
            el.child(
                div()
                    .flex_none()
                    .h(px(14.))
                    .w(px(28.))
                    .rounded(px(4.))
                    .bg(skeleton.opacity(0.7)),
            )
        })
}

fn filter_strip(
    harnesses: &[(String, String)],
    current_harness: &str,
    search: &Entity<InputState>,
    muted: Hsla,
    foreground: Hsla,
    primary: Hsla,
    accent: Hsla,
    is_dark: bool,
    loading: bool,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let border = theme.border;
    let underline = primary;

    let mut left = div()
        .flex()
        .items_center()
        .gap(px(2.))
        .min_w_0()
        .flex_1()
        .overflow_x_hidden();

    // Harness marks only: Pi · OpenCode · Google
    for (harness_id, display_name) in harnesses {
        let selected = harness_id == current_harness;
        let (mark, tint) = harness_mark(harness_id);
        let id = harness_id.clone();
        let tip = display_name.clone();
        let search = search.clone();
        let placeholder = format!("Search {display_name} models.");
        left = left.child(filter_icon(
            SharedString::from(format!("coder-agent-filter-harness-{id}")),
            mark.unwrap_or(VendorMark::Icon(PhosphorIcon::Cpu)),
            tint,
            selected,
            muted,
            foreground,
            accent,
            underline,
            tip,
            move |_, window, cx| {
                let _ = Bridge::halo_agent_set_harness(&id, cx);
                let _ = Bridge::halo_agent_set_picker_filter(None, cx);
                search.update(cx, |search, cx| {
                    search.set_placeholder(placeholder.clone(), window, cx);
                    search.focus(window, cx);
                });
            },
        ));
    }

    div()
        .id("coder-agent-picker-filters")
        .w_full()
        .flex_none()
        .px(px(8.))
        .pt(px(8.))
        .pb(px(4.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(4.))
        .border_b_1()
        .border_color(border.opacity(if is_dark { 0.6 } else { 1. }))
        .child(left)
        // Refresh doubles as the catalog's progress light: grid loader while
        // the probe runs, a button when it is idle.
        .child(if loading {
            div()
                .id("coder-agent-filter-refresh")
                .size(FILTER_HIT)
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .aria_label("Refreshing models")
                .child(Loader::grid_shift().size(px(13.)).color(muted))
                .into_any_element()
        } else {
            filter_icon(
                "coder-agent-filter-refresh",
                VendorMark::Icon(PhosphorIcon::ArrowClockwise),
                None,
                false,
                muted,
                foreground,
                accent,
                underline,
                "Refresh models",
                move |_, _, cx| {
                    let _ = Bridge::halo_agent_refresh_catalogs(cx);
                },
            )
            .into_any_element()
        })
}

fn filter_icon(
    id: impl Into<SharedString>,
    mark: VendorMark,
    tint: Option<Hsla>,
    active: bool,
    muted: Hsla,
    foreground: Hsla,
    accent: Hsla,
    underline: Hsla,
    tooltip: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let tip = tooltip.into();
    let glyph = render_mark(
        Some(mark),
        tint,
        px(14.),
        PhosphorIcon::Circle,
        if active { foreground } else { muted },
    );

    div()
        .id(id.into())
        .relative()
        .size(FILTER_HIT)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .hover(move |s| s.bg(accent))
        .when(active, |el| el.bg(accent.opacity(0.55)))
        .tooltip(move |window, cx| {
            celestia_ui::components::Tooltip::new(tip.clone()).build(window, cx)
        })
        .on_click(on_click)
        .child(glyph)
        .when(active, move |el| {
            el.child(
                div()
                    .absolute()
                    .bottom(px(2.))
                    .left(px(6.))
                    .right(px(6.))
                    .h(px(2.))
                    .rounded_full()
                    .bg(underline),
            )
        })
}

struct RowSpec {
    id: SharedString,
    title: SharedString,
    shortcut: Option<u8>,
    mark: Option<VendorMark>,
    tint: Option<Hsla>,
    selected: bool,
    fallback: PhosphorIcon,
}

#[derive(Clone, Copy)]
struct RowColors {
    accent: Hsla,
    foreground: Hsla,
    muted: Hsla,
    row_radius: Pixels,
}

/// Brand glyph for a harness/model: prefer an embedded SVG mark; otherwise a
/// Phosphor logo (or the caller's generic fallback when neither is known).
#[derive(Clone, Copy)]
pub(crate) enum VendorMark {
    Svg(&'static str),
    Icon(PhosphorIcon),
}

pub(crate) fn render_mark(
    mark: Option<VendorMark>,
    tint: Option<Hsla>,
    size: Pixels,
    fallback: PhosphorIcon,
    fallback_color: Hsla,
) -> AnyElement {
    match mark {
        Some(VendorMark::Svg(path)) => svg()
            .path(path)
            .size(size)
            .flex_none()
            .text_color(tint.unwrap_or(fallback_color))
            .into_any_element(),
        Some(VendorMark::Icon(icon)) => Phosphor::new(icon)
            .size(size)
            .color(tint.unwrap_or(fallback_color))
            .into_any_element(),
        None => Phosphor::new(fallback)
            .size(size)
            .color(tint.unwrap_or(fallback_color))
            .into_any_element(),
    }
}

fn model_row(spec: RowSpec, colors: RowColors) -> Stateful<Div> {
    let selected = spec.selected;
    let mark = render_mark(
        spec.mark,
        spec.tint,
        MARK_SIZE,
        spec.fallback,
        if spec.mark.is_some() {
            colors.foreground
        } else {
            colors.muted
        },
    );
    let shortcut_mod = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl"
    };

    div()
        .id(spec.id)
        .w_full()
        .h(ROW_HEIGHT)
        .flex_none()
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(colors.row_radius)
        .cursor_pointer()
        .hover(move |s| s.bg(colors.accent))
        .when(selected, |el| el.bg(colors.accent))
        .child(mark)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(if selected {
                    colors.foreground
                } else {
                    colors.foreground.opacity(0.92)
                })
                .child(spec.title),
        )
        .when_some(spec.shortcut, |el, n| {
            el.child(
                div()
                    .flex_none()
                    .px(px(5.))
                    .py(px(1.))
                    .rounded(px(4.))
                    .bg(colors.muted.opacity(0.18))
                    .text_xs()
                    .text_color(colors.muted)
                    .child(format!("{shortcut_mod} {n}")),
            )
        })
}

fn section_label(label: impl Into<SharedString>, muted: Hsla) -> impl IntoElement {
    div()
        .w_full()
        .px(px(8.))
        .pt(px(8.))
        .pb(px(2.))
        .text_xs()
        .text_color(muted)
        .child(label.into())
}

/// Model ids in shortcut order (first 9 visible rows).
fn shortcut_model_ids(models: &[(String, String)], query: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (_, group) in group_models_by_provider(models, query) {
        for (id, _) in group {
            if !out.contains(&id) {
                out.push(id);
            }
        }
    }
    out.truncate(9);
    out
}

/// Resolve ⌘/Ctrl+N against the current search.
fn pick_shortcut_from_state(state: &BridgeState, index: usize, query: &str) -> Option<String> {
    let models: Vec<(String, String)> = state
        .coder
        .agent_model_catalog
        .iter()
        .map(|m| (m.id.clone(), m.display_name.clone()))
        .collect();
    shortcut_model_ids(&models, query).get(index).cloned()
}

fn shortcut_for(ids: &[String], model_id: &str) -> Option<u8> {
    ids.iter()
        .position(|id| id == model_id)
        .map(|i| (i + 1) as u8)
}

fn group_models_by_provider(
    models: &[(String, String)],
    query: &str,
) -> Vec<(String, Vec<(String, String)>)> {
    let mut map: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (id, name) in models {
        if !matches_query(query, [name.as_str(), id.as_str()]) {
            continue;
        }
        let provider = model_provider(id).to_string();
        map.entry(provider)
            .or_default()
            .push((id.clone(), name.clone()));
    }
    map.into_iter().collect()
}

fn model_provider(id: &str) -> &str {
    id.split_once('/').map(|(p, _)| p).unwrap_or("other")
}

fn provider_label(provider: &str) -> String {
    match provider {
        "openai" => "OpenAI".into(),
        "anthropic" => "Anthropic".into(),
        "google" => "Google".into(),
        "deepseek" => "DeepSeek".into(),
        "groq" => "Groq".into(),
        "ollama" => "Ollama".into(),
        "minimax" => "Minimax".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                None => "Other".into(),
            }
        }
    }
}

/// Vendor mark for a harness/model. Prefer embedded SVG marks; otherwise Phosphor.
pub(super) fn model_mark(harness: &str, model_id: &str) -> (Option<VendorMark>, Option<Hsla>) {
    let provider = model_provider(model_id);
    if provider == "anthropic" || model_id.starts_with("anthropic/") {
        return (
            Some(VendorMark::Svg("harness-mark/claude-mark.svg")),
            Some(rgb(0xD97757).into()),
        );
    } else if provider == "openai" || model_id.starts_with("openai/") {
        return (Some(VendorMark::Svg("harness-mark/openai-mark.svg")), None);
    } else if provider == "google" || model_id.starts_with("google/") {
        return (
            Some(VendorMark::Icon(PhosphorIcon::GoogleLogo)),
            Some(rgb(0x4285F4).into()),
        );
    } else if provider == "deepseek" || model_id.starts_with("deepseek/") {
        return (
            Some(VendorMark::Icon(PhosphorIcon::Robot)),
            Some(rgb(0x4D6BFE).into()),
        );
    } else if provider == "groq" || model_id.starts_with("groq/") {
        return (None, Some(rgb(0xF59E0B).into()));
    } else if provider == "ollama" || model_id.starts_with("ollama") {
        return (None, Some(rgb(0x14B8A6).into()));
    } else if provider == "minimax" {
        return (
            Some(VendorMark::Icon(PhosphorIcon::Cube)),
            Some(rgb(0xA855F7).into()),
        );
    }

    // The id's own provider outranks the selected harness. A selection kept across
    // a harness switch is still `opencode/flint-40b-free`, so it has to keep
    // OpenCode's mark instead of inheriting Pi's the moment Pi is selected — the
    // mark changes only when a model is actually picked.
    let (provider_mark, provider_tint) = harness_mark(provider);
    if provider_mark.is_some() {
        return (provider_mark, provider_tint);
    }
    harness_mark(harness)
}

pub(crate) fn harness_mark(harness: &str) -> (Option<VendorMark>, Option<Hsla>) {
    match harness {
        "pi" => (
            Some(VendorMark::Svg("harness-mark/pi-mark.svg")),
            Some(rgb(0x14B8A6).into()),
        ),
        "opencode" => (
            Some(VendorMark::Svg("harness-mark/opencode-mark.svg")),
            Some(rgb(0xF97316).into()),
        ),
        "antigravity" => (
            Some(VendorMark::Svg("harness-mark/antigravity-mark.svg")),
            Some(rgb(0x4285F4).into()),
        ),
        "cursor" => (
            Some(VendorMark::Svg("harness-mark/cursor-mark.svg")),
            Some(rgb(0xE5E5E5).into()),
        ),
        _ => (None, None),
    }
}

fn matches_query(query: &str, haystacks: [&str; 2]) -> bool {
    query.is_empty() || haystacks.iter().any(|h| h.to_lowercase().contains(query))
}
