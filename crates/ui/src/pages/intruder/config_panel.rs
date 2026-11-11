//! Intruder config: Request / Attack / Payloads tabs.

use celestia_ui::components::HorizontalAlignment;
use celestia_ui::components::VStack;
use celestia_ui::components::composite::tab_bar::TabBarItem;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::input::Input;
use celestia_ui::components::primitive::textarea::Textarea;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{
    Bridge, BridgeState, IntruderAttackMode, IntruderConfigTab, IntruderPayloadType,
    IntruderProcessingStep,
};

use super::markers::{auto_mark_request, clear_markers};
use crate::pages::chrome::{content_tabs, panel};
use crate::pages::editors::ToolEditors;

pub fn render_config_panel(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let tab = state.intruder.config.config_tab;
    let muted = cx.theme().muted_foreground;

    panel(
        "Attack configuration",
        Some("Mark positions with §payload§, pick a mode, then start.".into()),
        VStack::new(HorizontalAlignment::Leading)
            .spacing(px(8.))
            .child(render_config_tabs(tab))
            .child(match tab {
                IntruderConfigTab::Request => {
                    render_request_tab(editors, muted, cx).into_any_element()
                }
                IntruderConfigTab::Attack => {
                    render_attack_tab(state, editors, muted, cx).into_any_element()
                }
                IntruderConfigTab::Payloads => {
                    render_payloads_tab(state, editors, muted, cx).into_any_element()
                }
            }),
        cx,
    )
}

fn render_config_tabs(active: IntruderConfigTab) -> impl IntoElement {
    content_tabs("intruder-config-tabs")
        .item(TabBarItem::new("request", "Request").closable(false))
        .item(TabBarItem::new("attack", "Attack").closable(false))
        .item(TabBarItem::new("payloads", "Payloads").closable(false))
        .active(match active {
            IntruderConfigTab::Request => "request",
            IntruderConfigTab::Attack => "attack",
            IntruderConfigTab::Payloads => "payloads",
        })
        .on_select(|id, _, cx| {
            let tab = match id.as_ref() {
                "attack" => IntruderConfigTab::Attack,
                "payloads" => IntruderConfigTab::Payloads,
                _ => IntruderConfigTab::Request,
            };
            Bridge::state(cx).update(cx, |s| s.intruder.config.config_tab = tab);
        })
}

fn render_request_tab(editors: &ToolEditors, muted: Hsla, _cx: &App) -> impl IntoElement {
    let url = editors.intruder_url.clone();
    let body = editors.intruder_body.clone();
    let headers = editors.intruder_headers.clone();

    VStack::new(HorizontalAlignment::Leading)
        .spacing(px(8.))
        .child(
            div()
                .flex()
                .gap_1()
                .child(
                    Button::new("intruder-clear-markers")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("Clear §")
                        .on_click({
                            let url = url.clone();
                            let body = body.clone();
                            let headers = headers.clone();
                            move |_, window, cx| {
                                url.update(cx, |s, cx| {
                                    let next = clear_markers(&s.value());
                                    s.set_value(next, window, cx);
                                });
                                body.update(cx, |s, cx| {
                                    let next = clear_markers(&s.value());
                                    s.set_value(next, window, cx);
                                });
                                headers.update(cx, |s, cx| {
                                    let next = clear_markers(&s.value());
                                    s.set_value(next, window, cx);
                                });
                            }
                        }),
                )
                .child(
                    Button::new("intruder-auto-mark")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("Auto-mark")
                        .on_click({
                            let url = url.clone();
                            let body = body.clone();
                            move |_, window, cx| {
                                let current_url = url.read(cx).value().to_string();
                                let current_body = body.read(cx).value().to_string();
                                let (next_url, next_body) =
                                    auto_mark_request(&current_url, &current_body);
                                url.update(cx, |s, cx| {
                                    s.set_value(next_url, window, cx);
                                });
                                body.update(cx, |s, cx| {
                                    s.set_value(next_body, window, cx);
                                });
                            }
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Wrap query / form / JSON values with §markers§"),
                ),
        )
        .child(
            div()
                .w_full()
                .flex()
                .gap_2()
                .child(
                    div()
                        .w(px(100.))
                        .child(Input::new(&editors.intruder_method)),
                )
                .child(div().flex_1().child(Input::new(&editors.intruder_url))),
        )
        .child(
            div()
                .w_full()
                .h(px(72.))
                .child(Textarea::new(&editors.intruder_headers)),
        )
        .child(
            div()
                .w_full()
                .h(px(100.))
                .child(Textarea::new(&editors.intruder_body)),
        )
}

fn render_attack_tab(
    state: &BridgeState,
    editors: &ToolEditors,
    muted: Hsla,
    cx: &App,
) -> impl IntoElement {
    let cfg = &state.intruder.config;
    let theme = cx.theme();

    VStack::new(HorizontalAlignment::Leading)
        .spacing(px(10.))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(muted)
                .child("Attack mode"),
        )
        .child(render_mode_cards(cfg.mode, theme.accent, muted))
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(stepper_row(
                    "Concurrency",
                    &format!("{}", cfg.concurrency),
                    muted,
                    |cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.intruder.config.concurrency =
                                s.intruder.config.concurrency.saturating_sub(1).max(1);
                        });
                    },
                    |cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.intruder.config.concurrency =
                                (s.intruder.config.concurrency + 1).min(200);
                        });
                    },
                ))
                .child(stepper_row(
                    "Delay ms",
                    &format!("{}", cfg.delay_ms),
                    muted,
                    |cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.intruder.config.delay_ms =
                                s.intruder.config.delay_ms.saturating_sub(50);
                        });
                    },
                    |cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.intruder.config.delay_ms =
                                s.intruder.config.delay_ms.saturating_add(50).min(10_000);
                        });
                    },
                )),
        )
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(muted)
                .child("Grep — Match"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(toggle_chip(
                    "intruder-grep-match",
                    "Enabled",
                    cfg.grep_match_enabled,
                    |cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.intruder.config.grep_match_enabled =
                                !s.intruder.config.grep_match_enabled;
                        });
                    },
                ))
                .child(toggle_chip(
                    "intruder-grep-case",
                    "Case sensitive",
                    cfg.grep_case_sensitive,
                    |cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.intruder.config.grep_case_sensitive =
                                !s.intruder.config.grep_case_sensitive;
                        });
                    },
                ))
                .child(
                    div()
                        .flex_1()
                        .child(Input::new(&editors.intruder_grep_keyword)),
                ),
        )
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(muted)
                .child("Grep — Extract"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(toggle_chip(
                    "intruder-grep-extract",
                    "Enabled",
                    cfg.grep_extract_enabled,
                    |cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.intruder.config.grep_extract_enabled =
                                !s.intruder.config.grep_extract_enabled;
                        });
                    },
                ))
                .child(
                    div()
                        .flex_1()
                        .child(Input::new(&editors.intruder_grep_regex)),
                ),
        )
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(muted)
                .child("Session handling"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(toggle_chip(
                    "intruder-session",
                    "Enabled",
                    cfg.session_enabled,
                    |cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.intruder.config.session_enabled = !s.intruder.config.session_enabled;
                        });
                    },
                ))
                .child(
                    div()
                        .flex_1()
                        .child(Input::new(&editors.intruder_session_token)),
                )
                .child(
                    div()
                        .flex_1()
                        .child(Input::new(&editors.intruder_session_header)),
                ),
        )
}

fn render_mode_cards(active: IntruderAttackMode, _accent: Hsla, muted: Hsla) -> impl IntoElement {
    let mut chips = div().w_full().flex().flex_wrap().gap_1();
    for mode in IntruderAttackMode::ALL {
        let selected = mode == active;
        let label = mode.label();
        chips = chips.child(
            Button::new(SharedString::from(format!("mode-{label}")))
                .variant(if selected {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Ghost
                })
                .size(ButtonSize::Small)
                .label(label)
                .tooltip(mode.description())
                .on_click(move |_, _, cx| {
                    Bridge::state(cx).update(cx, |s| s.intruder.config.mode = mode);
                }),
        );
    }
    div().w_full().flex().flex_col().gap_1().child(chips).child(
        div()
            .text_xs()
            .text_color(muted)
            .child(active.description()),
    )
}

fn render_payloads_tab(
    state: &BridgeState,
    editors: &ToolEditors,
    muted: Hsla,
    _cx: &App,
) -> impl IntoElement {
    let cfg = &state.intruder.config;

    VStack::new(HorizontalAlignment::Leading)
        .spacing(px(8.))
        .child(
            div()
                .flex()
                .gap_1()
                .children(IntruderPayloadType::ALL.into_iter().map(|ty| {
                    let selected = cfg.payload_type == ty;
                    Button::new(SharedString::from(format!("payload-type-{}", ty.label())))
                        .variant(if selected {
                            ButtonVariant::Secondary
                        } else {
                            ButtonVariant::Ghost
                        })
                        .size(ButtonSize::Small)
                        .label(ty.label())
                        .on_click(move |_, _, cx| {
                            Bridge::state(cx).update(cx, |s| s.intruder.config.payload_type = ty);
                        })
                })),
        )
        .child(match cfg.payload_type {
            IntruderPayloadType::SimpleList => div()
                .w_full()
                .h(px(140.))
                .child(Textarea::new(&editors.intruder_payloads))
                .into_any_element(),
            IntruderPayloadType::NumberRange => render_number_range(cfg, muted).into_any_element(),
        })
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(muted)
                .child("Payload processing"),
        )
        .child(div().flex().flex_wrap().gap_1().children(
            IntruderProcessingStep::ALL.into_iter().map(|step| {
                let selected = cfg.processing.contains(&step);
                Button::new(SharedString::from(format!("proc-{}", step.label())))
                    .variant(if selected {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Ghost
                    })
                    .size(ButtonSize::Small)
                    .label(step.label())
                    .on_click(move |_, _, cx| {
                        Bridge::state(cx).update(cx, |s| {
                            let list = &mut s.intruder.config.processing;
                            if let Some(pos) = list.iter().position(|s| *s == step) {
                                list.remove(pos);
                            } else {
                                list.push(step);
                            }
                        });
                    })
            }),
        ))
}

fn render_number_range(cfg: &halo_bridge::IntruderConfig, muted: Hsla) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_xs().text_color(muted).child(format!(
            "Range {} → {} step {}  format {}",
            cfg.number_start, cfg.number_end, cfg.number_step, cfg.number_format
        )))
        .child(
            div()
                .flex()
                .gap_2()
                .child(stepper_row(
                    "Start",
                    &format!("{}", cfg.number_start),
                    muted,
                    |cx| {
                        Bridge::state(cx).update(cx, |s| s.intruder.config.number_start -= 1);
                    },
                    |cx| {
                        Bridge::state(cx).update(cx, |s| s.intruder.config.number_start += 1);
                    },
                ))
                .child(stepper_row(
                    "End",
                    &format!("{}", cfg.number_end),
                    muted,
                    |cx| {
                        Bridge::state(cx).update(cx, |s| s.intruder.config.number_end -= 1);
                    },
                    |cx| {
                        Bridge::state(cx).update(cx, |s| s.intruder.config.number_end += 1);
                    },
                ))
                .child(stepper_row(
                    "Step",
                    &format!("{}", cfg.number_step),
                    muted,
                    |cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.intruder.config.number_step =
                                (s.intruder.config.number_step - 1).max(1);
                        });
                    },
                    |cx| {
                        Bridge::state(cx).update(cx, |s| s.intruder.config.number_step += 1);
                    },
                )),
        )
        .child(
            div()
                .flex()
                .gap_1()
                .children(["{}", "{:02}", "{:04}", "{:08}"].into_iter().map(|fmt| {
                    let selected = cfg.number_format == fmt;
                    Button::new(SharedString::from(format!("num-fmt-{fmt}")))
                        .variant(if selected {
                            ButtonVariant::Secondary
                        } else {
                            ButtonVariant::Ghost
                        })
                        .size(ButtonSize::Small)
                        .label(fmt)
                        .on_click(move |_, _, cx| {
                            Bridge::state(cx).update(cx, |s| {
                                s.intruder.config.number_format = fmt.to_string();
                            });
                        })
                })),
        )
}

fn stepper_row(
    label: &str,
    value: &str,
    muted: Hsla,
    on_dec: impl Fn(&mut App) + 'static,
    on_inc: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    let id_base = label.to_lowercase().replace(' ', "-");
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(div().text_xs().text_color(muted).child(label.to_string()))
        .child(
            Button::new(SharedString::from(format!("{id_base}-dec")))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .label("−")
                .tooltip(format!("Decrease {label}"))
                .on_click(move |_, _, cx| on_dec(cx)),
        )
        .child(
            div()
                .min_w(px(36.))
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .child(value.to_string()),
        )
        .child(
            Button::new(SharedString::from(format!("{id_base}-inc")))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .label("+")
                .tooltip(format!("Increase {label}"))
                .on_click(move |_, _, cx| on_inc(cx)),
        )
}

fn toggle_chip(
    id: &'static str,
    label: &'static str,
    on: bool,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .variant(if on {
            ButtonVariant::Secondary
        } else {
            ButtonVariant::Ghost
        })
        .size(ButtonSize::Small)
        .label(label)
        .on_click(move |_, _, cx| on_click(cx))
}
