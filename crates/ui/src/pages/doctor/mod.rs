//! Doctor page — diagnostic health checks and installers for agent harnesses,
//! language servers, and browser/proxy tools.

use celestia_ui::components::primitive::button::{
    Button, ButtonLoadingStyle, ButtonSize, ButtonVariant,
};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState, DoctorCategory, DoctorItem, DoctorStatus};

use crate::components::confirm_destructive;
use crate::pages::chrome::{PAGE_GAP, debug_panel};

/// Run one Doctor action, asking first unless the gate is off.
///
/// Every row here changes something outside the app's own data directory —
/// installs, ownership repairs, cache reclaims — so the click says what it is
/// about to do. The dialog states the row's diagnosis verbatim, which is also
/// the only place the affected paths are named.
pub(crate) fn request_fix(window: &mut Window, cx: &mut App, item_id: &str) {
    let (ask, title, detail, action) = describe_action(cx, item_id);
    let id = item_id.to_string();
    if !ask {
        Bridge::doctor_install_item(&id, cx);
        return;
    }
    confirm_destructive(window, cx, title, detail, action, move |_, cx| {
        Bridge::doctor_install_item(&id, cx);
    });
}

/// Run every pending fix as one batch, asking once.
pub(crate) fn request_fix_all(window: &mut Window, cx: &mut App, count: usize) {
    let ask = Bridge::state(cx)
        .read(cx)
        .coder_settings
        .confirm_system_changes;
    if !ask {
        Bridge::doctor_install_all_missing(cx);
        return;
    }
    let noun = if count == 1 { "issue" } else { "issues" };
    let detail = format!(
        "Halo will install or repair {count} {noun}. Each step is verified afterwards, and any \
         action that requires administrator privileges will prompt for your permission."
    );
    confirm_destructive(
        window,
        cx,
        format!("Fix {count} {noun}?"),
        detail,
        "Fix all",
        move |_, cx| {
            Bridge::doctor_install_all_missing(cx);
        },
    );
}

/// `(ask_first, title, description, action_label)` for one row.
fn describe_action(cx: &App, item_id: &str) -> (bool, String, String, String) {
    let state = Bridge::state(cx);
    let state = state.read(cx);
    let item = state.doctor.items.iter().find(|item| item.id == item_id);
    let action = item
        .map(|item| item.action_label.clone())
        .unwrap_or_else(|| "Fix".to_string());
    let name = item
        .map(|item| item.name.clone())
        .unwrap_or_else(|| item_id.to_string());
    let detail = item
        .map(|item| format!("{} {}", item.description, item.hint))
        .unwrap_or_else(|| "This changes files outside the app.".to_string());
    // Ownership repairs ask macOS for administrator rights; say so before the
    // password prompt appears, so the second dialog is not a surprise.
    let detail = if escalates(item_id) {
        format!("{detail} macOS will ask for your password to change ownership in place.")
    } else {
        detail
    };
    (
        state.coder_settings.confirm_system_changes,
        format!("{action} — {name}?"),
        detail,
        action,
    )
}

/// Actions that may show the macOS administrator prompt.
fn escalates(item_id: &str) -> bool {
    matches!(item_id, "system:agent-state" | "system:instance-locks")
}

pub fn render_doctor(state: &BridgeState, cx: &App) -> impl IntoElement {
    let doctor = &state.doctor;
    let checking = doctor.checking;
    let batch_running = doctor.batch_running;
    let missing_installable = doctor.auto_installable_missing_ids().len();
    let confirm_system_changes = state.coder_settings.confirm_system_changes;
    // One busy flag for the whole page: the active row spins, every other
    // action button greys out, so no click looks ignored.
    let busy = doctor.busy();

    div()
        .id("doctor-page")
        .size_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .px_6()
        .py_4()
        .overflow_y_scrollbar()
        .child(render_header(
            doctor,
            checking,
            batch_running,
            missing_installable,
            confirm_system_changes,
            busy,
            cx,
        ))
        .child(render_summary_banner(doctor, cx))
        .child(render_filter_strip(doctor, cx))
        .child(render_items_list(doctor, cx))
        .child(render_activity_log(doctor, cx))
        .child(debug_panel("doctor-page", "Doctor"))
}

fn render_header(
    _doctor: &halo_bridge::DoctorView,
    checking: bool,
    batch_running: bool,
    missing_installable: usize,
    confirm_system_changes: bool,
    busy: bool,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;

    div()
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .pb_3()
        .border_b_1()
        .border_color(theme.border.opacity(0.5))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Phosphor::new(PhosphorIcon::FirstAid)
                                .size(px(20.))
                                .color(theme.accent),
                        )
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::NORMAL)
                                .child("Doctor"),
                        ),
                )
                .child(div().text_xs().text_color(muted).child(
                    "System health, missing dependency detection, and automated installers.",
                )),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("doctor-confirm-toggle")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(if confirm_system_changes {
                            PhosphorIcon::CheckCircle
                        } else {
                            PhosphorIcon::Circle
                        })
                        .tooltip(if confirm_system_changes {
                            "Every fix asks for confirmation before it runs"
                        } else {
                            "Fixes run immediately — no confirmation"
                        })
                        .label(if confirm_system_changes {
                            "Ask first"
                        } else {
                            "No prompt"
                        })
                        .on_click(|_, _, cx| {
                            let next = !Bridge::state(cx)
                                .read(cx)
                                .coder_settings
                                .confirm_system_changes;
                            Bridge::set_confirm_system_changes(next, cx);
                        }),
                )
                .child(
                    Button::new("doctor-refresh-btn")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::ArrowClockwise)
                        .label(if checking {
                            "Checking…"
                        } else {
                            "Re-check All"
                        })
                        .disabled(busy)
                        .loading(checking)
                        .loading_style(ButtonLoadingStyle::GridShift)
                        .on_click(|_, _, cx| {
                            Bridge::doctor_refresh(cx);
                        }),
                )
                .child(
                    Button::new("doctor-install-all-btn")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::DownloadSimple)
                        .label(if batch_running {
                            "Fixing…".to_string()
                        } else if missing_installable > 0 {
                            format!("Fix All Issues ({missing_installable})")
                        } else {
                            "All Healthy".to_string()
                        })
                        .disabled((busy && !batch_running) || missing_installable == 0)
                        .loading(batch_running)
                        .loading_style(ButtonLoadingStyle::GridShift)
                        .on_click(move |_, window, cx| {
                            request_fix_all(window, cx, missing_installable);
                        }),
                ),
        )
}

fn render_summary_banner(doctor: &halo_bridge::DoctorView, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let ready = doctor.ready_count();
    let upgrade = doctor.upgrade_count();
    let missing = doctor.missing_count();
    let total = doctor.items.len();

    let (headline, headline_color, banner_bg) = if missing == 0 && upgrade == 0 && total > 0 {
        (
            "All system dependencies are healthy and ready to use.",
            theme.success,
            theme.success.opacity(0.08),
        )
    } else if missing > 0 {
        (
            "Some checks are failing. Every failing row carries a one-click fix — or use Fix All Issues.",
            theme.warning,
            theme.warning.opacity(0.08),
        )
    } else if upgrade > 0 {
        (
            "Updates and cleanups are available for installed tools.",
            theme.accent,
            theme.accent.opacity(0.08),
        )
    } else {
        (
            "Checking environment dependencies…",
            theme.muted_foreground,
            theme.muted.opacity(0.3),
        )
    };

    div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_4()
        .p_3()
        .rounded(theme.radius)
        .bg(banner_bg)
        .border_1()
        .border_color(headline_color.opacity(0.25))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().size(px(8.)).rounded_full().bg(headline_color))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(headline),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(counter_badge("Ready", ready, theme.success, cx))
                .when(upgrade > 0, |el| {
                    el.child(counter_badge("Upgrade Needed", upgrade, theme.warning, cx))
                })
                .when(missing > 0, |el| {
                    el.child(counter_badge("Missing", missing, theme.danger, cx))
                }),
        )
}

fn counter_badge(label: &str, count: usize, color: Hsla, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .items_center()
        .gap_1p5()
        .px_2()
        .py_0p5()
        .rounded(theme.radius_sm())
        .bg(theme.background.opacity(0.8))
        .border_1()
        .border_color(color.opacity(0.3))
        .child(div().size(px(6.)).rounded_full().bg(color))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.foreground)
                .child(count.to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label.to_string()),
        )
}

fn render_filter_strip(doctor: &halo_bridge::DoctorView, _cx: &App) -> impl IntoElement {
    let active_cat = doctor.filter;
    let mut strip = div().flex().flex_wrap().gap_2().w_full();

    for cat in DoctorCategory::ALL {
        let selected = cat == active_cat;
        let count = match cat {
            DoctorCategory::All => doctor.items.len(),
            c => doctor.items.iter().filter(|i| i.category == c).count(),
        };
        let label = format!("{} ({count})", cat.label());
        let cat_id = cat;

        strip = strip.child(
            Button::new(SharedString::from(format!("doctor-cat-{}", cat.id())))
                .variant(if selected {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                })
                .size(ButtonSize::Small)
                .label(label)
                .on_click(move |_, _, cx| {
                    Bridge::doctor_set_filter(cat_id, cx);
                }),
        );
    }

    strip
}

fn render_items_list(doctor: &halo_bridge::DoctorView, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let items = doctor.filtered_items();

    let mut list = div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.06))
        .p_2();

    if items.is_empty() {
        list = list.child(
            div()
                .w_full()
                .p_6()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::NORMAL)
                        .child("No dependencies found for this category."),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Click Re-check All to refresh the environment."),
                ),
        );
    } else {
        for item in items {
            list = list.child(render_item_row(item, doctor, cx));
        }
    }

    list
}

fn render_item_row(
    item: &DoctorItem,
    doctor: &halo_bridge::DoctorView,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let installing_this = doctor.is_installing(&item.id);
    // Every button on the page reflects *any* action in flight, so a click can
    // never look ignored: this row spins, the others grey out.
    let busy = doctor.busy();
    let id = item.id.clone();
    // The outcome of the last run for this row, shown where the click happened
    // instead of only in the activity log at the bottom of the page.
    let last_log = doctor
        .last_log_for(&item.id)
        .map(|entry| (entry.message.clone(), entry.is_error));
    // Armed: the first click only asked, this row is waiting for the second.
    let armed = doctor.is_armed(&item.id);

    let (status_label, status_color, status_icon) = match item.status {
        DoctorStatus::Ready => ("Ready", theme.success, PhosphorIcon::CheckCircle),
        DoctorStatus::NeedsUpgrade => {
            ("Upgrade Needed", theme.warning, PhosphorIcon::ArrowCircleUp)
        }
        DoctorStatus::Missing => ("Missing", muted, PhosphorIcon::XCircle),
        DoctorStatus::Failed => ("Failed", theme.danger, PhosphorIcon::WarningCircle),
    };

    let detail_line = if let Some(path) = &item.resolved_path {
        if let Some(ver) = &item.version {
            format!("{ver} · {path}")
        } else {
            path.clone()
        }
    } else {
        item.hint.clone()
    };

    div()
        .id(SharedString::from(format!("doctor-row-{}", item.id)))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .px_3()
        .py_2p5()
        .rounded(theme.radius_sm())
        .border_b_1()
        .border_color(theme.border.opacity(0.4))
        .hover(|s| s.bg(theme.muted.opacity(0.2)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.foreground)
                                .child(item.name.clone()),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded(theme.radius_xs())
                                .bg(theme.muted)
                                .text_xs()
                                .font_family("monospace")
                                .text_color(theme.muted_foreground)
                                .child(item.command.clone()),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(item.description.clone()),
                )
                // The hint carries the diagnosis ("what exactly is wrong"),
                // which is the whole point of a failing row — the action label
                // on the right is not a substitute for it.
                .when(item.status != DoctorStatus::Ready, |el| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(status_color)
                            .child(item.hint.clone()),
                    )
                })
                // Outcome of the last run, right under the diagnosis: the click
                // reports back where it happened, not only in the log at the
                // bottom of the page.
                .when_some(last_log.clone(), |el, (message, is_error)| {
                    if !is_error && item.status == DoctorStatus::Ready {
                        return el;
                    }
                    el.child(
                        div()
                            .text_xs()
                            .text_color(if is_error { theme.danger } else { muted })
                            .child(message),
                    )
                })
                .when(armed, |el| {
                    el.child(div().text_xs().text_color(theme.warning).child(
                        "Nothing has run yet — click Confirm to change ownership on this \
                                 machine.",
                    ))
                })
                .child(
                    div()
                        .text_xs()
                        .text_color(if item.status == DoctorStatus::Ready {
                            muted
                        } else {
                            status_color
                        })
                        .font_family("monospace")
                        .child(detail_line),
                ),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px_2()
                        .py_1()
                        .rounded(theme.radius_sm())
                        .bg(status_color.opacity(0.12))
                        .child(Phosphor::new(status_icon).size(px(14.)).color(status_color))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(status_color)
                                .child(if installing_this {
                                    "Installing…".to_string()
                                } else {
                                    status_label.to_string()
                                }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .when_some(item.doc_url.clone(), |el, url| {
                            let url_clone = url.clone();
                            el.child(
                                Button::new(SharedString::from(format!("doctor-doc-{}", item.id)))
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Small)
                                    .leading_icon(PhosphorIcon::ArrowSquareOut)
                                    .tooltip(format!("Open {url}"))
                                    .label("Docs")
                                    .on_click(move |_, _, cx| {
                                        cx.open_url(&url_clone);
                                    }),
                            )
                        })
                        .when(item.can_auto_install, |el| {
                            let action_id = id.clone();
                            let label = if installing_this {
                                "Working…".to_string()
                            } else if armed {
                                "Confirm".to_string()
                            } else {
                                item.action_label.clone()
                            };

                            el.child(
                                Button::new(SharedString::from(format!("doctor-act-{}", item.id)))
                                    .variant(if armed {
                                        ButtonVariant::Destructive
                                    } else if item.status == DoctorStatus::Ready {
                                        ButtonVariant::Ghost
                                    } else {
                                        ButtonVariant::Primary
                                    })
                                    .size(ButtonSize::Small)
                                    .label(label)
                                    // Never disable the button that is loading:
                                    // `disabled` cancels the loading styling, so
                                    // the running action would look dead.
                                    .disabled(busy && !installing_this)
                                    .loading(installing_this)
                                    .loading_style(ButtonLoadingStyle::GridShift)
                                    .on_click(move |_, _, cx| {
                                        // The confirmation lives on the button:
                                        // the first click arms it, the second
                                        // one runs. A modal in between could
                                        // swallow the click and look dead.
                                        if armed {
                                            Bridge::doctor_disarm(cx);
                                            Bridge::doctor_install_item(&action_id, cx);
                                            return;
                                        }
                                        let gated = Bridge::state(cx)
                                            .read(cx)
                                            .coder_settings
                                            .confirm_system_changes;
                                        if gated {
                                            Bridge::doctor_arm_item(&action_id, cx);
                                        } else {
                                            Bridge::doctor_install_item(&action_id, cx);
                                        }
                                    }),
                            )
                        })
                        // No action for this row: the diagnosis above is the
                        // whole message, so nothing is repeated on the right.
                        .when(
                            !item.can_auto_install && item.status != DoctorStatus::Ready,
                            |el| {
                                el.child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .max_w(px(150.))
                                        .text_right()
                                        .child("Needs a manual step"),
                                )
                            },
                        ),
                ),
        )
}

fn render_activity_log(doctor: &halo_bridge::DoctorView, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let logs = &doctor.logs;

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .pt_2()
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child("Diagnostic & Installation Activity"),
                )
                .child(
                    Button::new("doctor-clear-logs")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Trash)
                        .label("Clear Logs")
                        .disabled(logs.is_empty())
                        .on_click(|_, _, cx| {
                            Bridge::doctor_clear_logs(cx);
                        }),
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(140.))
                .overflow_y_scrollbar()
                .rounded(theme.radius)
                .bg(theme.muted.opacity(0.35))
                .border_1()
                .border_color(theme.border)
                .p_2p5()
                .flex()
                .flex_col()
                .gap_1()
                .children(if logs.is_empty() {
                    vec![
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("No recent installation logs. Click Re-check All or run an install action.")
                            .into_any_element(),
                    ]
                } else {
                    logs.iter()
                        .map(|entry| {
                            let color = if entry.is_error {
                                theme.danger
                            } else {
                                theme.foreground
                            };
                            div()
                                .flex()
                                .items_start()
                                .gap_2()
                                .text_xs()
                                .font_family("monospace")
                                .child(
                                    div()
                                        .text_color(muted)
                                        .flex_none()
                                        .child(format!("[{}]", entry.timestamp)),
                                )
                                .child(
                                    div()
                                        .text_color(color)
                                        .flex_1()
                                        .child(entry.message.clone()),
                                )
                                .into_any_element()
                        })
                        .collect()
                }),
        )
}
