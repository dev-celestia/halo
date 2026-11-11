//! Interactive Whiteboard / Drawing Studio dialog for sketches and diagrams.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

pub fn render_drawing_dialog(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let active_note_id = state.notes_active_id.clone();

    div()
        .id("drawing-dialog-overlay")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.background.opacity(0.65))
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            let _ = Bridge::set_notes_drawing_open(false, cx);
        })
        .child(
            div()
                .id("drawing-dialog-panel")
                .role(Role::Dialog)
                .aria_label("Drawing Canvas Studio")
                .w(px(860.))
                .max_w(relative(0.96))
                .h(px(620.))
                .max_h(relative(0.94))
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
                .on_mouse_down(MouseButton::Left, |_, _, _| {})
                // Header
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_6()
                        .py_3p5()
                        .border_b_1()
                        .border_color(theme.border)
                        .bg(theme.muted.opacity(0.1))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .size(px(24.))
                                        .rounded_md()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(theme.primary.opacity(0.15))
                                        .text_color(theme.primary)
                                        .child(PhosphorIcon::PaintBrush),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::NORMAL)
                                                .text_color(theme.foreground)
                                                .child("Drawing Studio Whiteboard"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .child("Create quick architecture diagrams, flowcharts, or sketches"),
                                        ),
                                ),
                        )
                        .child(
                            Button::new("drawing-close-btn")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::X)
                                .tooltip("Close Studio")
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::set_notes_drawing_open(false, cx);
                                }),
                        ),
                )
                // Whiteboard Toolbar
                .child(
                    div()
                        .w_full()
                        .px_6()
                        .py_2()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .border_b_1()
                        .border_color(theme.border)
                        .bg(theme.muted.opacity(0.04))
                        // Tool buttons
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new("tool-select")
                                        .variant(ButtonVariant::Secondary)
                                        .size(ButtonSize::Small)
                                        .leading_icon(PhosphorIcon::Cursor)
                                        .label("Select")
                                        .on_click(|_, _, _| {}),
                                )
                                .child(
                                    Button::new("tool-pen")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::Small)
                                        .leading_icon(PhosphorIcon::PencilSimple)
                                        .label("Pen")
                                        .on_click(|_, _, _| {}),
                                )
                                .child(
                                    Button::new("tool-rect")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::Small)
                                        .leading_icon(PhosphorIcon::Square)
                                        .label("Box")
                                        .on_click(|_, _, _| {}),
                                )
                                .child(
                                    Button::new("tool-circle")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::Small)
                                        .leading_icon(PhosphorIcon::Circle)
                                        .label("Circle")
                                        .on_click(|_, _, _| {}),
                                )
                                .child(
                                    Button::new("tool-arrow")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::Small)
                                        .leading_icon(PhosphorIcon::ArrowRight)
                                        .label("Arrow")
                                        .on_click(|_, _, _| {}),
                                )
                                .child(
                                    Button::new("tool-text")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::Small)
                                        .leading_icon(PhosphorIcon::TextT)
                                        .label("Text")
                                        .on_click(|_, _, _| {}),
                                ),
                        )
                        // Palette swatches
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .child(color_swatch(rgb(0x3b82f6), true))
                                .child(color_swatch(rgb(0x10b981), false))
                                .child(color_swatch(rgb(0xf59e0b), false))
                                .child(color_swatch(rgb(0xef4444), false))
                                .child(color_swatch(rgb(0x8b5cf6), false))
                                .child(color_swatch(rgb(0x64748b), false)),
                        ),
                )
                // Whiteboard canvas surface
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .relative()
                        .bg(theme.background)
                        .child(
                            canvas(
                                |_bounds, _window, _cx| {},
                                |bounds, _, window, cx| {
                                    let theme = cx.theme();
                                    // Paint subtle grid pattern
                                    let mut y = bounds.origin.y;
                                    while y < bounds.origin.y + bounds.size.height {
                                        window.paint_quad(fill(
                                            Bounds {
                                                origin: point(bounds.origin.x, y),
                                                size: size(bounds.size.width, px(1.)),
                                            },
                                            theme.border.opacity(0.12),
                                        ));
                                        y += px(24.);
                                    }
                                    let mut x = bounds.origin.x;
                                    while x < bounds.origin.x + bounds.size.width {
                                        window.paint_quad(fill(
                                            Bounds {
                                                origin: point(x, bounds.origin.y),
                                                size: size(px(1.), bounds.size.height),
                                            },
                                            theme.border.opacity(0.12),
                                        ));
                                        x += px(24.);
                                    }

                                    // Draw sample diagram shapes
                                    let center_x = bounds.origin.x + bounds.size.width / 2.0;
                                    let center_y = bounds.origin.y + bounds.size.height / 2.0;

                                    // Architecture box 1
                                    window.paint_quad(fill(
                                        Bounds {
                                            origin: point(center_x - px(160.), center_y - px(60.)),
                                            size: size(px(120.), px(60.)),
                                        },
                                        theme.primary.opacity(0.15),
                                    ));

                                    // Architecture box 2
                                    window.paint_quad(fill(
                                        Bounds {
                                            origin: point(center_x + px(40.), center_y - px(60.)),
                                            size: size(px(120.), px(60.)),
                                        },
                                        theme.muted.opacity(0.4),
                                    ));
                                },
                            )
                            .size_full(),
                        )
                        .child(
                            div()
                                .absolute()
                                .bottom_4()
                                .left_1_2()
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_3()
                                .py_1p5()
                                .rounded_full()
                                .bg(theme.popover.opacity(0.95))
                                .border_1()
                                .border_color(theme.border)
                                .shadow(vec![BoxShadow {
                                    offset: point(px(0.), px(4.)),
                                    blur_radius: px(12.),
                                    spread_radius: px(0.),
                                    inset: false,
                                    color: hsla(0., 0., 0., 0.15),
                                }])
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(theme.muted_foreground)
                                        .child("Whiteboard Canvas • Ready to export diagram"),
                                ),
                        ),
                )
                // Footer
                .child(
                    div()
                        .w_full()
                        .px_6()
                        .py_3()
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_t_1()
                        .border_color(theme.border)
                        .bg(theme.muted.opacity(0.1))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Diagram template will be inserted as Mermaid/Markdown into active note"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    Button::new("drawing-cancel-btn")
                                        .variant(ButtonVariant::Outline)
                                        .size(ButtonSize::Small)
                                        .label("Cancel")
                                        .on_click(|_, _, cx| {
                                            let _ = Bridge::set_notes_drawing_open(false, cx);
                                        }),
                                )
                                .child(
                                    Button::new("drawing-insert-btn")
                                        .variant(ButtonVariant::Primary)
                                        .size(ButtonSize::Small)
                                        .leading_icon(PhosphorIcon::PlusCircle)
                                        .label("Insert Diagram into Note")
                                        .on_click(move |_, _, cx| {
                                            if let Some(id) = &active_note_id {
                                                let (name, updated_content) = {
                                                    let state = Bridge::state(cx).read(cx);
                                                    if let Some(note) = state.notes.iter().find(|n| &n.id == id) {
                                                        let diagram = "\n\n```mermaid\ngraph TD\n    A[Client Request] --> B[Halo Proxy]\n    B --> C[Target Server]\n    C --> B\n    B --> D[Response Inspector]\n```\n";
                                                        (Some(note.name.clone()), Some(format!("{}{diagram}", note.note)))
                                                    } else {
                                                        (None, None)
                                                    }
                                                };
                                                if let (Some(name), Some(content)) = (name, updated_content) {
                                                    let _ = Bridge::save_note(id, &name, &content, cx);
                                                }
                                            }
                                            let _ = Bridge::set_notes_drawing_open(false, cx);
                                        }),
                                ),
                        ),
                ),
        )
}

fn color_swatch(color: Rgba, active: bool) -> impl IntoElement {
    div()
        .size(px(18.))
        .rounded_full()
        .bg(color)
        .cursor_pointer()
        .border_2()
        .border_color(if active {
            rgb(0xffffff)
        } else {
            rgba(0x00000022)
        })
}
