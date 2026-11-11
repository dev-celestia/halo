//! First-run setup dialog. Shown until onboarding is completed.
//!
//! Four stages: machine checks, appearance, a setup list (proxy CA, Node.js,
//! Halo Agent), then done. Chrome follows the shell popover — scrim, dialog,
//! theme tokens, and the shared buttons. The scrim does not dismiss.

use celestia_ui::components::primitive::button::{
    Button, ButtonLoadingStyle, ButtonSize, ButtonVariant,
};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::{ActiveTheme as _, ThemeMode};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState, DoctorItem, DoctorStatus};

use crate::theme::{self, SwitchingTarget};

const STEP_COUNT: u8 = 4;
const STEP_LOOK: u8 = 1;
const STEP_SETUP: u8 = 2;
const STAGES: [&str; STEP_COUNT as usize] = ["This Mac", "Look", "Set up", "Done"];

pub(crate) fn render_onboarding_dialog(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let step = state.onboarding_step.min(STEP_COUNT - 1);

    div()
        .id("onboarding-overlay")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.background.opacity(0.60))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("onboarding-dialog")
                .role(Role::Dialog)
                .aria_label("Set up Halo")
                .w(px(640.))
                .max_w(relative(0.94))
                .max_h(relative(0.92))
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded(theme.radius_xl())
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .text_color(theme.popover_foreground)
                .shadow(vec![BoxShadow {
                    offset: point(px(0.), px(18.)),
                    blur_radius: px(48.),
                    spread_radius: px(0.),
                    inset: false,
                    color: hsla(0., 0., 0., 0.35),
                }])
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(header(step, cx))
                .child(body(step, state, cx))
                .child(footer(step, state, cx)),
        )
}

fn header(step: u8, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(20.))
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_end()
                .gap(px(16.))
                .children(STAGES.iter().enumerate().map(|(index, name)| {
                    let active = index as u8 == step;
                    let reached = (index as u8) <= step;
                    div()
                        .pb(px(10.))
                        .pt(px(12.))
                        .border_b_2()
                        .border_color(if active {
                            theme.foreground
                        } else {
                            theme.border.opacity(0.)
                        })
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(if reached {
                            theme.foreground
                        } else {
                            theme.muted_foreground
                        })
                        .child(*name)
                })),
        )
        .child(
            div()
                .id("onboard-close")
                .mb(px(2.))
                .size(px(24.))
                .flex_none()
                .rounded(theme.radius_sm())
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .text_color(theme.muted_foreground)
                .hover(|s| s.bg(theme.muted.opacity(0.4)).text_color(theme.foreground))
                .role(Role::Button)
                .aria_label("Dismiss onboarding")
                .on_click(|_, _, cx| {
                    Bridge::complete_onboarding(cx);
                })
                .child(
                    Phosphor::new(PhosphorIcon::X)
                        .size(px(12.))
                        .color(theme.muted_foreground),
                ),
        )
}

fn body(step: u8, state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let (title, blurb) = step_copy(step);
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .gap(px(16.))
        .px(px(20.))
        .py(px(16.))
        .overflow_y_scrollbar()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("Step {} of {STEP_COUNT}", step + 1)),
                )
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(title),
                )
                .child(
                    div()
                        .text_sm()
                        .line_height(relative(1.45))
                        .text_color(theme.muted_foreground)
                        .child(blurb),
                ),
        )
        .child(step_body(step, state, cx))
}

fn step_copy(step: u8) -> (&'static str, &'static str) {
    match step {
        0 => (
            "Check this Mac",
            "Halo looks at the processor, Node.js, local storage, and the app bundle before setup.",
        ),
        STEP_LOOK => (
            "Choose how Halo looks",
            "Pick light or dark. You can change this anytime in Settings.",
        ),
        STEP_SETUP => (
            "Set up Halo",
            "Trusting the proxy CA is optional. macOS may ask for your password, so that stays a separate click. Set up installs Node.js and Halo Agent when they are missing.",
        ),
        _ => (
            "You're set",
            "Theme, CA, and agent preferences live in Settings. Open tools from the dock anytime.",
        ),
    }
}

fn step_body(step: u8, state: &BridgeState, cx: &App) -> AnyElement {
    match step {
        0 => machine_checks(state, cx).into_any_element(),
        STEP_LOOK => appearance_controls(cx).into_any_element(),
        STEP_SETUP => setup_checks(state, cx).into_any_element(),
        _ => div().into_any_element(),
    }
}

fn machine_checks(state: &BridgeState, cx: &App) -> impl IntoElement {
    checklist(
        cx,
        vec![
            arch_row(cx).into_any_element(),
            doctor_row(state, "runtime:node", "Node.js", cx).into_any_element(),
            doctor_row(state, "system:storage", "Local storage", cx).into_any_element(),
            doctor_row(state, "system:app-bundle", "App", cx).into_any_element(),
        ],
    )
}

fn arch_row(cx: &App) -> impl IntoElement {
    let detail = match std::env::consts::ARCH {
        "aarch64" => "Apple silicon".to_string(),
        other => other.to_string(),
    };
    status_row("onboard-arch", "Architecture", detail, Mark::Ready, cx)
}

fn setup_checks(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let ready = ready_count(state);
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(checklist(
            cx,
            vec![
                ca_row(state, cx).into_any_element(),
                node_row(state, cx).into_any_element(),
                pi_row(state, cx).into_any_element(),
            ],
        ))
        .child(
            div()
                .w_full()
                .flex()
                .justify_end()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!("{ready} of 3")),
        )
}

fn appearance_controls(cx: &App) -> impl IntoElement {
    let is_dark = cx.theme().is_dark();
    let target = theme::switching_target(cx);
    let switching = !matches!(target, SwitchingTarget::None);
    let light_pending = matches!(target, SwitchingTarget::Mode(ThemeMode::Light));
    let dark_pending = matches!(target, SwitchingTarget::Mode(ThemeMode::Dark));
    div()
        .flex()
        .flex_wrap()
        .gap(px(8.))
        .child(
            Button::new("onboard-theme-light")
                .variant(if !is_dark {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Outline
                })
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Sun)
                .label("Light")
                .loading(light_pending)
                .loading_style(ButtonLoadingStyle::GridShift)
                .disabled(switching && !light_pending)
                .on_click(|_, _, cx| {
                    theme::change_mode(ThemeMode::Light, cx);
                }),
        )
        .child(
            Button::new("onboard-theme-dark")
                .variant(if is_dark {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Outline
                })
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Moon)
                .label("Dark")
                .loading(dark_pending)
                .loading_style(ButtonLoadingStyle::GridShift)
                .disabled(switching && !dark_pending)
                .on_click(|_, _, cx| {
                    theme::change_mode(ThemeMode::Dark, cx);
                }),
        )
}

fn ca_row(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let busy = state.ca_busy;
    let (detail, mark) = match doctor_item(state, "proxy:mitm-ca") {
        Some(item) => (item_detail(item), mark_for(item.status, busy)),
        None if state.doctor.checking || state.doctor.items.is_empty() => {
            ("Checking…".to_string(), Mark::Busy)
        }
        None => ("Not checked".to_string(), Mark::Idle),
    };
    let trust_label = if busy {
        "Working…"
    } else if cfg!(target_os = "macos") {
        "Trust in Keychain"
    } else {
        "Trust in Chrome"
    };

    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .px(px(12.))
        .py(px(10.))
        .child(status_text("Trust the proxy CA", detail, mark, cx))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.))
                .pl(px(26.))
                .child(
                    Button::new("onboard-ca-trust")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Key)
                        .label(trust_label)
                        .disabled(busy)
                        .loading(busy)
                        .loading_style(ButtonLoadingStyle::GridShift)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::trust_ca(cx);
                        }),
                )
                .child(
                    Button::new("onboard-ca-export")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::DownloadSimple)
                        .label(if busy {
                            "Saving…"
                        } else {
                            "Download certificate"
                        })
                        .disabled(busy)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::export_ca_cert(cx);
                        }),
                ),
        )
        .when_some(state.last_error.clone(), |el, err| {
            el.child(
                div()
                    .pl(px(26.))
                    .text_xs()
                    .text_color(theme.danger)
                    .child(err),
            )
        })
        .when(
            state.last_error.is_none() && state.last_message.is_some(),
            |el| {
                el.child(
                    div()
                        .pl(px(26.))
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(state.last_message.clone().unwrap_or_default()),
                )
            },
        )
}

fn node_row(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let row = doctor_row(state, "runtime:node", "Node.js", cx);
    let error = state
        .doctor
        .last_log_for("runtime:node")
        .filter(|entry| entry.is_error)
        .map(|entry| entry.message.clone());
    div()
        .flex()
        .flex_col()
        .child(row)
        .when_some(error, |el, err| {
            el.child(
                div()
                    .px(px(12.))
                    .pb(px(10.))
                    .pl(px(38.))
                    .text_xs()
                    .text_color(theme.danger)
                    .child(err),
            )
        })
}

fn pi_row(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let installed = state.coder.agent_harness_installed;
    let needs_upgrade = state.coder.agent_pi_needs_upgrade;
    let installing = state.coder.agent_installing.as_deref() == Some("pi");
    let hint = state.coder.agent_pi_setup_hint.clone().unwrap_or_else(|| {
        "Install @earendil-works/pi-coding-agent 1.4.0 to use Halo Agent.".into()
    });
    let (detail, mark) = if installing {
        ("Installing Pi 1.4.0…".to_string(), Mark::Busy)
    } else if installed && !needs_upgrade {
        ("Ready".to_string(), Mark::Ready)
    } else if needs_upgrade {
        (hint, Mark::Warn)
    } else {
        (hint, Mark::Idle)
    };
    let error = (state.coder.agent_install_error_harness.as_deref() == Some("pi"))
        .then(|| state.coder.agent_install_error.clone())
        .flatten();

    div()
        .flex()
        .flex_col()
        .child(status_row("onboard-pi", "Halo Agent", detail, mark, cx))
        .when_some(error, |el, err| {
            el.child(
                div()
                    .px(px(12.))
                    .pb(px(10.))
                    .pl(px(38.))
                    .text_xs()
                    .text_color(theme.danger)
                    .child(err),
            )
        })
}

fn doctor_row(state: &BridgeState, id: &str, title: &'static str, cx: &App) -> AnyElement {
    let row_id = SharedString::from(format!("onboard-{id}"));
    match doctor_item(state, id) {
        Some(item) => {
            let busy = state.doctor.is_installing(id);
            let detail = if busy {
                "Installing…".to_string()
            } else {
                item_detail(item)
            };
            status_row(row_id, title, detail, mark_for(item.status, busy), cx).into_any_element()
        }
        None if state.doctor.checking || state.doctor.items.is_empty() => {
            status_row(row_id, title, "Checking…".to_string(), Mark::Busy, cx).into_any_element()
        }
        None => {
            status_row(row_id, title, "Not checked".to_string(), Mark::Idle, cx).into_any_element()
        }
    }
}

fn footer(step: u8, state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let is_last = step + 1 >= STEP_COUNT;
    let label = primary_label(step, state);
    let waiting = step == STEP_SETUP && (checks_pending(state) || setup_busy(state));

    div()
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .px(px(16.))
        .py(px(12.))
        .border_t_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .when(step > 0, |el| {
                    el.child(
                        Button::new("onboard-back")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Small)
                            .leading_icon(PhosphorIcon::CaretLeft)
                            .label("Back")
                            .on_click(move |_, _, cx| {
                                Bridge::set_onboarding_step(step.saturating_sub(1), cx);
                            }),
                    )
                })
                .child(
                    Button::new("onboard-skip")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("Not now")
                        .on_click(|_, _, cx| {
                            Bridge::complete_onboarding(cx);
                        }),
                ),
        )
        .child(
            Button::new("onboard-primary")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Small)
                .label(label)
                .loading(waiting && !is_last)
                .loading_style(ButtonLoadingStyle::GridShift)
                .disabled(waiting && !is_last)
                .on_click(move |_, _, cx| primary_action(step, cx)),
        )
}

fn primary_label(step: u8, state: &BridgeState) -> &'static str {
    if step + 1 >= STEP_COUNT {
        "Start using Halo"
    } else if step == STEP_SETUP
        && (checks_pending(state) || node_outstanding(state) || pi_outstanding(state))
    {
        "Set up"
    } else {
        "Continue"
    }
}

fn primary_action(step: u8, cx: &mut App) {
    if step + 1 >= STEP_COUNT {
        Bridge::complete_onboarding(cx);
        return;
    }
    if step == STEP_SETUP {
        let (pending, busy, outstanding) = {
            let state = Bridge::state(cx);
            let state = state.read(cx);
            (
                checks_pending(state),
                setup_busy(state),
                node_outstanding(state) || pi_outstanding(state),
            )
        };
        if pending || busy {
            return;
        }
        if outstanding {
            run_setup(cx);
            return;
        }
    }
    Bridge::set_onboarding_step(step.saturating_add(1).min(STEP_COUNT - 1), cx);
}

fn run_setup(cx: &mut App) {
    let (install_node, install_pi) = {
        let state = Bridge::state(cx);
        let state = state.read(cx);
        let install_node = node_outstanding(state) && !state.doctor.is_installing("runtime:node");
        let install_pi =
            pi_outstanding(state) && state.coder.agent_installing.as_deref() != Some("pi");
        (install_node, install_pi)
    };
    if install_node {
        Bridge::doctor_install_item("runtime:node", cx);
    }
    if install_pi {
        let _ = Bridge::halo_agent_install_pi(cx);
    }
}

fn checks_pending(state: &BridgeState) -> bool {
    state.doctor.items.is_empty()
}

fn node_outstanding(state: &BridgeState) -> bool {
    match doctor_item(state, "runtime:node") {
        Some(item) => item.status != DoctorStatus::Ready,
        None => false,
    }
}

fn pi_outstanding(state: &BridgeState) -> bool {
    !state.coder.agent_harness_installed || state.coder.agent_pi_needs_upgrade
}

fn setup_busy(state: &BridgeState) -> bool {
    state.doctor.is_installing("runtime:node")
        || state.coder.agent_installing.as_deref() == Some("pi")
}

fn ready_count(state: &BridgeState) -> usize {
    let ca = doctor_item(state, "proxy:mitm-ca").is_some_and(|item| item.status.is_healthy());
    let node = doctor_item(state, "runtime:node").is_some_and(|item| item.status.is_healthy());
    let pi = !pi_outstanding(state);
    usize::from(ca) + usize::from(node) + usize::from(pi)
}

fn doctor_item<'a>(state: &'a BridgeState, id: &str) -> Option<&'a DoctorItem> {
    state.doctor.items.iter().find(|item| item.id == id)
}

fn item_detail(item: &DoctorItem) -> String {
    match item.status {
        DoctorStatus::Ready => match (&item.version, &item.resolved_path) {
            (Some(version), Some(path)) => format!("{version} · {path}"),
            (Some(version), None) => version.clone(),
            (None, Some(path)) => path.clone(),
            (None, None) => {
                if item.hint.is_empty() {
                    "Ready".to_string()
                } else {
                    item.hint.clone()
                }
            }
        },
        DoctorStatus::Missing if item.id == "runtime:node" => "Not installed".to_string(),
        DoctorStatus::NeedsUpgrade => item
            .version
            .clone()
            .filter(|version| !version.is_empty())
            .unwrap_or_else(|| {
                if item.hint.is_empty() {
                    "Upgrade needed".to_string()
                } else {
                    item.hint.clone()
                }
            }),
        DoctorStatus::Missing | DoctorStatus::Failed => {
            if item.hint.is_empty() {
                item.status.label().to_string()
            } else {
                item.hint.clone()
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Mark {
    Ready,
    Warn,
    Idle,
    Busy,
    Danger,
}

fn mark_for(status: DoctorStatus, busy: bool) -> Mark {
    if busy {
        return Mark::Busy;
    }
    match status {
        DoctorStatus::Ready => Mark::Ready,
        DoctorStatus::NeedsUpgrade => Mark::Warn,
        DoctorStatus::Missing => Mark::Idle,
        DoctorStatus::Failed => Mark::Danger,
    }
}

fn mark_icon(mark: Mark, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let (icon, color) = match mark {
        Mark::Ready => (PhosphorIcon::CheckCircle, theme.success),
        Mark::Warn => (PhosphorIcon::WarningCircle, theme.warning),
        Mark::Danger => (PhosphorIcon::XCircle, theme.danger),
        Mark::Busy => (PhosphorIcon::CircleNotch, theme.muted_foreground),
        Mark::Idle => (PhosphorIcon::Circle, theme.muted_foreground),
    };
    Phosphor::new(icon).size(px(16.)).color(color)
}

fn status_text(title: &str, detail: String, mark: Mark, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .items_start()
        .gap(px(10.))
        .child(div().mt(px(1.)).flex_none().child(mark_icon(mark, cx)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .line_height(relative(1.4))
                        .text_color(theme.muted_foreground)
                        .child(detail),
                ),
        )
}

fn status_row(
    id: impl Into<ElementId>,
    title: &str,
    detail: String,
    mark: Mark,
    cx: &App,
) -> impl IntoElement {
    div()
        .id(id)
        .w_full()
        .px(px(12.))
        .py(px(10.))
        .child(status_text(title, detail, mark, cx))
}

fn checklist(cx: &App, rows: Vec<AnyElement>) -> impl IntoElement {
    let theme = cx.theme();
    let mut list = div()
        .w_full()
        .flex()
        .flex_col()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .overflow_hidden();
    for (index, row) in rows.into_iter().enumerate() {
        list = list.child(
            div()
                .w_full()
                .when(index > 0, |el| el.border_t_1().border_color(theme.border))
                .child(row),
        );
    }
    list
}
