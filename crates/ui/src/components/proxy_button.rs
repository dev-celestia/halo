//! Reusable proxy start / stop / toggle controls.
//!
//! Prefer these helpers over calling [`Bridge::start_proxy`] / [`Bridge::stop_proxy`]
//! directly from UI so busy-state handling stays consistent.

use celestia_ui::components::primitive::button::{
    Button, ButtonLoadingStyle, ButtonSize, ButtonVariant,
};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

/// Snapshot of proxy running / transition state for button chrome.
#[derive(Clone, Copy, Debug)]
pub struct ProxyActionState {
    pub running: bool,
    pub busy: bool,
}

impl ProxyActionState {
    pub fn from_bridge(state: &BridgeState) -> Self {
        Self {
            running: state.proxy.running,
            busy: state.proxy_busy,
        }
    }

    pub fn from_app(cx: &App) -> Self {
        Self::from_bridge(Bridge::state(cx).read(cx))
    }

    pub fn starting(self) -> bool {
        self.busy && !self.running
    }

    pub fn stopping(self) -> bool {
        self.busy && self.running
    }
}

/// Label verbosity for start / stop buttons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProxyButtonCopy {
    /// "Start" / "Stop"
    Compact,
    /// "Start proxy" / "Stop proxy"
    #[default]
    Full,
}

impl ProxyButtonCopy {
    fn start(self, starting: bool) -> &'static str {
        match (self, starting) {
            (Self::Compact, true) => "Starting…",
            (Self::Compact, false) => "Start",
            (Self::Full, true) => "Starting…",
            (Self::Full, false) => "Start proxy",
        }
    }

    fn stop(self, stopping: bool) -> &'static str {
        match (self, stopping) {
            (Self::Compact, true) => "Stopping…",
            (Self::Compact, false) => "Stop",
            (Self::Full, true) => "Stopping…",
            (Self::Full, false) => "Stop proxy",
        }
    }
}

/// Start the proxy using the preferred ports from bridge state.
pub fn start_proxy(cx: &mut App) {
    let (http, tls) = {
        let state = Bridge::state(cx).read(cx);
        (state.preferred_http_port, state.preferred_tls_port)
    };
    let _ = Bridge::start_proxy(http, tls, cx);
}

/// Stop the running proxy.
pub fn stop_proxy(cx: &mut App) {
    let _ = Bridge::stop_proxy(cx);
}

/// Toggle the proxy on/off, no-op while a start/stop is in flight.
pub fn toggle_proxy(cx: &mut App) {
    let state = ProxyActionState::from_app(cx);
    if state.busy {
        return;
    }
    if state.running {
        stop_proxy(cx);
    } else {
        start_proxy(cx);
    }
}

/// Single button that starts when stopped and stops when running (widget footer).
pub fn proxy_toggle_button(
    id: impl Into<SharedString>,
    state: ProxyActionState,
    copy: ProxyButtonCopy,
) -> Button {
    let id = id.into();
    if state.running {
        Button::new(format!("{id}-stop"))
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::XSmall)
            .label(copy.stop(state.stopping()))
            .disabled(state.busy)
            .loading(state.stopping())
            .loading_style(ButtonLoadingStyle::GridShift)
            .on_click(|_, _, cx| stop_proxy(cx))
    } else {
        Button::new(format!("{id}-start"))
            .variant(ButtonVariant::Primary)
            .size(ButtonSize::XSmall)
            .label(copy.start(state.starting()))
            .disabled(state.busy)
            .loading(state.starting())
            .loading_style(ButtonLoadingStyle::GridShift)
            .on_click(|_, _, cx| start_proxy(cx))
    }
}

/// Explicit start button (settings / toolbars).
pub fn proxy_start_button(
    id: impl Into<ElementId>,
    state: ProxyActionState,
    copy: ProxyButtonCopy,
) -> Button {
    Button::new(id)
        .variant(ButtonVariant::Primary)
        .size(ButtonSize::Small)
        .leading_icon(PhosphorIcon::Play)
        .label(copy.start(state.starting()))
        .disabled(state.running || state.stopping())
        .loading(state.starting())
        .loading_style(ButtonLoadingStyle::GridShift)
        .on_click(|_, _, cx| start_proxy(cx))
}

/// Explicit stop button (settings / toolbars).
pub fn proxy_stop_button(
    id: impl Into<ElementId>,
    state: ProxyActionState,
    copy: ProxyButtonCopy,
) -> Button {
    Button::new(id)
        .variant(ButtonVariant::Secondary)
        .size(ButtonSize::Small)
        .leading_icon(PhosphorIcon::Stop)
        .label(copy.stop(state.stopping()))
        .disabled(!state.running || state.starting())
        .loading(state.stopping())
        .loading_style(ButtonLoadingStyle::GridShift)
        .on_click(|_, _, cx| stop_proxy(cx))
}

/// Dock control: the proxy status lives on a primary button that toggles it.
pub fn proxy_dock_button(state: ProxyActionState, label: impl Into<SharedString>) -> Button {
    let label = label.into();
    let tip: SharedString = if state.starting() {
        "Proxy is starting".into()
    } else if state.stopping() {
        "Proxy is stopping".into()
    } else if state.running {
        "Stop the proxy".into()
    } else {
        "Start the proxy".into()
    };
    // Green while the proxy is up; Secondary (dim) while it's off.
    Button::new("dock-proxy")
        .variant(if state.running {
            ButtonVariant::Success
        } else {
            ButtonVariant::Secondary
        })
        .size(ButtonSize::XSmall)
        .leading_icon(if state.running {
            PhosphorIcon::Stop
        } else {
            PhosphorIcon::Play
        })
        .label(label)
        .loading(state.busy)
        .loading_style(ButtonLoadingStyle::GridShift)
        .tooltip(tip)
        .on_click(|_, _, cx| toggle_proxy(cx))
}
