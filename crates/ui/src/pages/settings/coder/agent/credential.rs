//! Credential card, approval-mode dropdown, and choice blocks.

use super::*;

pub(super) fn credential_card(
    provider: &'static str,
    description: &'static str,
    current_val: Option<&str>,
    input_entity: &Entity<celestia_ui::components::primitive::input::InputState>,
    apply_id: &'static str,
    clear_id: &'static str,
    apply_label: &'static str,
    on_apply: impl Fn(Option<String>, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let warning = theme.warning;
    let is_configured = current_val.is_some_and(|v| !v.trim().is_empty());
    let input = input_entity.clone();
    let on_apply = std::sync::Arc::new(on_apply);
    let on_apply_clear = std::sync::Arc::clone(&on_apply);

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .px_3()
        .py_3()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_start()
                .gap_2()
                .child(
                    Phosphor::new(PhosphorIcon::Key)
                        .size(px(14.))
                        .color(if is_configured {
                            theme.success
                        } else {
                            warning
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(foreground)
                                        .child(provider),
                                )
                                .child(if is_configured {
                                    div()
                                        .text_xs()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded(theme.radius_sm())
                                        .bg(theme.accent)
                                        .text_color(theme.primary)
                                        .child("Configured")
                                } else {
                                    div()
                                        .text_xs()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded(theme.radius_sm())
                                        .bg(theme.muted)
                                        .text_color(muted)
                                        .child("Not set")
                                }),
                        )
                        .child(div().text_xs().text_color(muted).child(description)),
                ),
        )
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .px_2()
                        .py_1()
                        .rounded(theme.radius_sm())
                        .border_1()
                        .border_color(theme.border)
                        .child(Input::new(input_entity).appearance(false).cleanable(true)),
                )
                .child(
                    Button::new(apply_id)
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Check)
                        .label(apply_label)
                        .on_click({
                            let on_apply = std::sync::Arc::clone(&on_apply);
                            let input = input.clone();
                            move |_, window, cx| {
                                let val = input.read(cx).value().trim().to_string();
                                let opt = if val.is_empty() { None } else { Some(val) };
                                on_apply(opt, cx);
                                input.update(cx, |state, cx| {
                                    state.set_value("", window, cx);
                                });
                            }
                        }),
                )
                .when(is_configured, |el| {
                    el.child(
                        Button::new(clear_id)
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Small)
                            .label("Clear")
                            .on_click(move |_, _, cx| {
                                on_apply_clear(None, cx);
                            }),
                    )
                }),
        )
}

pub(super) fn approval_mode_dropdown(selected: AgentApprovalMode) -> impl IntoElement {
    dropdown_select(
        "coder-agent-approval-select",
        selected.title(),
        move |menu, _, _| {
            let mut menu = menu;
            for mode in AgentApprovalMode::ALL {
                let label = if mode.is_default() {
                    format!("{} (Default)", mode.title())
                } else {
                    mode.title().to_string()
                };
                menu = menu.item(
                    PopupMenuItem::new(label)
                        .checked(mode == selected)
                        .on_click(move |_, _, cx| {
                            Bridge::update_coder_settings(cx, move |s| {
                                s.agent_approval_mode = mode;
                            });
                        }),
                );
            }
            menu
        },
    )
}

/// Label + description above a chip row (the lists can wrap, so they do not
/// fit the right-aligned [`row`] layout).
pub(super) fn choice_block(
    label: &'static str,
    chips: impl IntoElement,
    hint: Option<String>,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let mut block = div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_sm().child(label))
        .child(chips);
    if let Some(hint) = hint {
        block = block.child(div().text_xs().text_color(muted).child(hint));
    }
    block
}
