//! Interactive Commit Graph DAG component.
//!
//! Renders multi-lane topological tracks with smooth Bézier curves, branch ref pills,
//! commit summaries, author avatars, short hashes, and timestamps.

use celestia_ui::components::traits::Size;
use celestia_ui::theme::ActiveTheme as _;
use chrono::{Local, TimeZone as _};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState, GitGraphCommit, GitGraphRef, GitGraphRefKind};

use super::soft_chip;
use crate::pages::source_control::author_avatar;

const ROW_HEIGHT: f32 = 26.0;
/// Avatar (20) + gap + short name — keep tight so Description can flex.
const AUTHOR_COL: f32 = 112.0;
/// Seven-char short SHA in mono `text_xs`.
const COMMIT_COL: f32 = 64.0;
/// Fits `29 Oct 2026 23:03` in mono `text_xs`.
const DATE_COL: f32 = 132.0;
const LANE_STEP: f32 = 20.0;
const LANE_X0: f32 = 14.0;

fn lane_x(lane: usize) -> f32 {
    LANE_X0 + lane as f32 * LANE_STEP
}

fn lane_color(lane: usize) -> Rgba {
    const HEXES: [u32; 8] = [
        0xf59e0bff, // Amber
        0x06b6d4ff, // Cyan
        0xa855f7ff, // Purple
        0x10b981ff, // Green
        0xec4899ff, // Pink
        0x3b82f6ff, // Blue
        0xf97316ff, // Orange
        0x14b8a6ff, // Teal
    ];
    rgba(HEXES[lane % HEXES.len()])
}

pub fn render_commit_graph(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let graph = &state.git_graph;
    let commits = graph.filtered_commits();
    let selected_sha = graph.selected_commit.as_deref();

    div()
        .id("sc-commit-graph")
        .size_full()
        .flex()
        .flex_col()
        .bg(theme.background)
        // Table header — normal weight at text_xs.
        .child(
            div()
                .h(px(26.))
                .flex()
                .flex_row()
                .items_center()
                .bg(theme.muted.opacity(0.2))
                .border_b_1()
                .border_color(theme.border)
                .px_4()
                .gap_2()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.muted_foreground)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family(theme.mono_font_family.clone())
                        .child("Graph & Description"),
                )
                .child(div().w(px(AUTHOR_COL)).child("Author"))
                .child(div().w(px(COMMIT_COL)).child("Commit"))
                .child(div().w(px(DATE_COL)).child("Date")),
        )
        // Virtual / scrollable commit rows
        .child(
            div()
                .id("sc-graph-rows-scroll")
                .flex_1()
                .overflow_y_scrollbar()
                .when(commits.is_empty(), |d| {
                    d.child(
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .p_6()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("No commits match the filter."),
                    )
                })
                .children(commits.into_iter().enumerate().map(|(idx, commit)| {
                    let is_selected = selected_sha == Some(commit.sha.as_str());
                    render_commit_row(commit, idx, is_selected, theme)
                })),
        )
}

fn render_commit_row(
    commit: &GitGraphCommit,
    idx: usize,
    is_selected: bool,
    theme: &celestia_ui::theme::Theme,
) -> impl IntoElement {
    let sha_clone = commit.sha.clone();
    let is_head = commit.refs.iter().any(|r| r.is_head);

    div()
        .id(("graph-row", idx))
        .h(px(ROW_HEIGHT))
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .px_4()
        .gap_2()
        .cursor_pointer()
        .border_b_1()
        .border_color(theme.border.opacity(0.35))
        .when(is_selected, |d| {
            d.bg(theme.primary.opacity(0.14))
                .border_color(theme.primary.opacity(0.35))
        })
        .when(!is_selected, |d| {
            d.hover(|h| h.bg(theme.muted.opacity(0.25)))
        })
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            let _ = Bridge::git_graph_select_commit(&sha_clone, cx);
        })
        // Column 1: DAG Graph & Description
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .flex_row()
                .items_center()
                .gap_2p5()
                .min_w_0()
                .child(render_dag_tracks(commit, is_head, theme))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .children(commit.refs.iter().map(|r| render_ref_pill(r, theme)))
                        .child(
                            div()
                                .text_xs()
                                .font_family(theme.mono_font_family.clone())
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.foreground)
                                .truncate()
                                .child(commit.subject.clone()),
                        ),
                ),
        )
        // Column 2: Author
        .child(
            div()
                .w(px(AUTHOR_COL))
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .min_w_0()
                .child(author_avatar(
                    commit.author_name.clone(),
                    &commit.author_email,
                    Size::Size(px(16.)),
                ))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .child(commit.author_name.clone()),
                ),
        )
        // Column 3: SHA
        .child(
            div()
                .w(px(COMMIT_COL))
                .text_xs()
                .font_family(theme.mono_font_family.clone())
                .text_color(theme.muted_foreground)
                .child(commit.short_sha.clone()),
        )
        // Column 4: Date — width sized to the stamp; Description keeps the flex remainder
        .child(
            div()
                .w(px(DATE_COL))
                .text_xs()
                .font_family(theme.mono_font_family.clone())
                .text_color(theme.muted_foreground)
                .whitespace_nowrap()
                .child(format_commit_date(commit.timestamp)),
        )
}

fn render_dag_tracks(
    commit: &GitGraphCommit,
    is_head: bool,
    theme: &celestia_ui::theme::Theme,
) -> impl IntoElement {
    let max_lane = commit
        .lane
        .max(commit.parent_lanes.iter().copied().max().unwrap_or(0))
        .max(commit.active_lanes.len().saturating_sub(1));
    let track_width = lane_x(max_lane) + LANE_STEP;

    let commit_lane = commit.lane;
    let has_incoming = commit.has_incoming;
    let active_lanes = commit.active_lanes.clone();
    let parent_lanes = commit.parent_lanes.clone();

    div()
        .relative()
        .h(px(ROW_HEIGHT))
        .w(px(track_width))
        .flex_shrink_0()
        .child(
            canvas(
                move |_, _, _| (),
                move |bounds, _, window, _| {
                    let ox = bounds.origin.x;
                    let oy = bounds.origin.y;
                    let cy = ROW_HEIGHT / 2.0;
                    let h = ROW_HEIGHT;
                    let stroke_w = px(2.2);

                    // 1. Straight active lanes passing through this row
                    for (lane_idx, slot) in active_lanes.iter().enumerate() {
                        if slot.is_some() && lane_idx != commit_lane {
                            let color = lane_color(lane_idx);
                            let lx = lane_x(lane_idx);
                            let mut b = PathBuilder::stroke(stroke_w);
                            b.move_to(point(ox + px(lx), oy));
                            b.line_to(point(ox + px(lx), oy + px(h)));
                            if let Ok(path) = b.build() {
                                window.paint_path(path, color);
                            }
                        }
                    }

                    // 2. Incoming line from above (only if commit has incoming connection)
                    if has_incoming {
                        let color = lane_color(commit_lane);
                        let cx_val = lane_x(commit_lane);
                        let mut b = PathBuilder::stroke(stroke_w);
                        b.move_to(point(ox + px(cx_val), oy));
                        b.line_to(point(ox + px(cx_val), oy + px(cy)));
                        if let Ok(path) = b.build() {
                            window.paint_path(path, color);
                        }
                    }

                    // 3. Outgoing lines to parent lanes below
                    for &parent_lane in &parent_lanes {
                        let color = lane_color(parent_lane);
                        let commit_x = lane_x(commit_lane);
                        let parent_x = lane_x(parent_lane);

                        if parent_lane == commit_lane {
                            // Same lane: straight vertical line downwards
                            let mut b = PathBuilder::stroke(stroke_w);
                            b.move_to(point(ox + px(commit_x), oy + px(cy)));
                            b.line_to(point(ox + px(commit_x), oy + px(h)));
                            if let Ok(path) = b.build() {
                                window.paint_path(path, color);
                            }
                        } else {
                            // Cross-lane branch/merge: smooth cubic Bézier S-curve
                            let mut b = PathBuilder::stroke(stroke_w);
                            b.move_to(point(ox + px(commit_x), oy + px(cy)));
                            let c1_y = cy + (h - cy) * 0.6;
                            let c2_y = h - (h - cy) * 0.6;
                            b.cubic_bezier_to(
                                point(ox + px(parent_x), oy + px(h)),
                                point(ox + px(commit_x), oy + px(c1_y)),
                                point(ox + px(parent_x), oy + px(c2_y)),
                            );
                            if let Ok(path) = b.build() {
                                window.paint_path(path, color);
                            }
                        }
                    }
                },
            )
            .size_full(),
        )
        // 4. Commit node circle on top of lines
        .child({
            let node_color = lane_color(commit.lane);
            let cx_val = lane_x(commit.lane);
            let radius = 4.0;
            let d = radius * 2.0;
            let cy = ROW_HEIGHT / 2.0;

            div()
                .absolute()
                .top(px(cy - radius))
                .left(px(cx_val - radius))
                .size(px(d))
                .rounded_full()
                .bg(node_color)
                .border_2()
                .border_color(if is_head {
                    theme.foreground
                } else {
                    theme.background
                })
        })
}

fn render_ref_pill(ref_item: &GitGraphRef, theme: &celestia_ui::theme::Theme) -> impl IntoElement {
    let color = match ref_item.kind {
        GitGraphRefKind::Head => theme.warning,
        GitGraphRefKind::LocalBranch => theme.primary,
        GitGraphRefKind::RemoteBranch => theme.info,
        GitGraphRefKind::Tag => theme.success,
    };
    soft_chip(ref_item.name.clone(), color, theme)
}

/// Local wall-clock stamp matching the status-bar clock (`HH:MM` with a colon).
pub(super) fn format_commit_date(timestamp: i64) -> String {
    if timestamp == 0 {
        return "—".to_string();
    }
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|local| local.format("%d %b %Y %H:%M").to_string())
        .unwrap_or_else(|| "—".to_string())
}
