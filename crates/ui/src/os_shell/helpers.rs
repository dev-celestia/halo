//! Shared helpers for the desktop shell.

use std::sync::Arc;

use celestia_ui::components::Chip;
use celestia_ui::components::StatDeltaTone;
use celestia_ui::components::primitive::image_icon::{ImageIcon, ImageIconShape, ImageIconSize};
use celestia_ui::components::traits::Sizable as _;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{AppFlag, BridgeState};

use crate::app::HaloApp;
use crate::nav::Page;
use crate::pages::chrome::StatusTone;

/// Smallest Chip size for experimental apps. Empty for Release.
pub(super) fn alpha_chip(page: Page, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div().when(page.flag() == Some(AppFlag::Alpha), |this| {
        this.child(
            Chip::new("app-flag-alpha", "ALPHA")
                .xsmall()
                .bg(theme.danger)
                .text_color(theme.danger_foreground),
        )
    })
}

#[allow(dead_code)]
pub(super) fn mark(id: impl Into<ElementId>, image: Arc<Image>, size: ImageIconSize) -> ImageIcon {
    ImageIcon::src(id, image)
        .size(size)
        .shape(ImageIconShape::Rounded)
        .bordered(false)
}

/// Open `page` as a work tab of the main shell (dock / start menu / desktop).
/// Closes the start menu on the shell that launched it. Satellite sub windows
/// are opened explicitly via right-click → “Open in New Sub Window”.
pub(super) fn open_app(weak: &WeakEntity<HaloApp>, page: Page) -> impl Fn(&mut App) + 'static {
    let weak = weak.clone();
    move |cx| {
        if let Some(app) = weak.upgrade() {
            app.update(cx, |this, cx| {
                this.close_launcher(cx);
                this.open_page(page, cx);
            });
        }
    }
}

pub(super) fn proxy_status(state: &BridgeState) -> (&'static str, StatusTone) {
    if state.proxy_busy {
        (
            if state.proxy.running {
                "Proxy stopping…"
            } else {
                "Proxy starting…"
            },
            StatusTone::Busy,
        )
    } else if state.proxy.running {
        ("PROXY ON", StatusTone::Active)
    } else {
        ("PROXY OFF", StatusTone::Idle)
    }
}

pub(super) fn proxy_metric(state: &BridgeState) -> (String, String, StatDeltaTone) {
    if state.proxy_busy {
        return ("…".into(), "Working".into(), StatDeltaTone::Neutral);
    }
    if state.proxy.running {
        let port = state
            .proxy
            .port
            .unwrap_or(state.proxy.default_port)
            .to_string();
        (port, "Listening".into(), StatDeltaTone::Up)
    } else {
        ("Off".into(), "Stopped".into(), StatDeltaTone::Neutral)
    }
}

pub(super) fn is_running(page: Page, state: &BridgeState) -> bool {
    match page {
        Page::Desktop => false,
        Page::HttpHistory => false,
        Page::Intercept => state.intercept.enabled || state.session_paused_len() > 0,
        Page::Repeater => state.any_repeater_response(),
        Page::Intruder => state.intruder_overview().running,
        Page::Crawler => state.crawler_overview().session.is_some(),
        Page::Coder => state.coder.root_path.is_some(),
        Page::SourceControl => state.coder.root_path.is_some() && !state.git.not_a_repo,
        // Only a live run — leftover transcript must not look like the tab is
        // selected while Coder (or anything else) is in front.
        Page::Agent => state.coder.agent_running,
        Page::Doctor => {
            state.doctor.checking
                || state.doctor.batch_running
                || !state.doctor.installing_ids.is_empty()
        }
        Page::Notes => !state.notes_open_tab_ids.is_empty(),
        Page::Diagram => false,
        Page::ApiMock => state.mock.server.running,
        Page::Jwt => false,
        Page::PortScanner => state.port_scanner.is_running,
        Page::Comparer => state.comparer.has_diff(),
        Page::FileExplorer => false,
        Page::R2Storage => state.r2.busy || state.r2.loading,
        Page::Wordlists => state.wordlists.loading,
        Page::SplitView => state.split_view.filled_count() > 0,
        Page::ApiOverride => state.mock.override_routes().iter().any(|r| r.enabled),
        Page::Settings => false,
    }
}

pub(super) fn badge_for(page: Page, state: &BridgeState) -> Option<String> {
    match page {
        Page::Intercept => count_badge(state.session_paused_len()),
        Page::Notes => count_badge(state.notes_open_tab_ids.len()),
        Page::Doctor => count_badge(state.doctor.missing_count()),
        Page::SourceControl => {
            let n = state.git.staged.len() + state.git.unstaged.len() + state.git.untracked.len();
            count_badge(n)
        }
        _ => None,
    }
}

fn count_badge(count: usize) -> Option<String> {
    match count {
        0 => None,
        1..=9 => Some(count.to_string()),
        _ => Some("9+".into()),
    }
}
