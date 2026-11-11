//! Diagram studio — flowchart nodes, connectors, and tables on a pan/zoom canvas.

mod router;
mod shapes;
mod table;

use std::sync::{Arc, Mutex};

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{
    Bridge, BridgeState, Camera2D, DiagramEdgeRoute, DiagramShapeKind, DiagramTool, DiagramWork,
    Port,
};

use crate::pages::editors::ToolEditors;

const GRID: f32 = 20.0;

/// Drag bookkeeping that must survive re-renders (pages are pure render fns).
static DRAG_OFFSET: Mutex<(f32, f32)> = Mutex::new((0.0, 0.0));
static PAN_LAST: Mutex<Option<(f32, f32)>> = Mutex::new(None);

#[derive(Clone, Copy)]
enum Hit {
    Node(usize),
    Table(usize),
}

fn hit_test(work: &DiagramWork, wx: f32, wy: f32) -> Option<Hit> {
    for (i, t) in work.tables.iter().enumerate().rev() {
        let (w, h) = table::table_size(t);
        if wx >= t.x && wx <= t.x + w && wy >= t.y && wy <= t.y + h {
            return Some(Hit::Table(i));
        }
    }
    for (i, n) in work.nodes.iter().enumerate().rev() {
        if wx >= n.x && wx <= n.x + n.width && wy >= n.y && wy <= n.y + n.height {
            return Some(Hit::Node(i));
        }
    }
    None
}

fn snap(v: f32, on: bool) -> f32 {
    if on { (v / GRID).round() * GRID } else { v }
}

fn auto_ports(a: (f32, f32), b: (f32, f32)) -> (Port, Port) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            (Port::East, Port::West)
        } else {
            (Port::West, Port::East)
        }
    } else if dy >= 0.0 {
        (Port::South, Port::North)
    } else {
        (Port::North, Port::South)
    }
}

pub fn render_diagram(
    state: &BridgeState,
    _editors: &ToolEditors,
    _window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let theme = cx.theme();
    let work = state.diagram.clone();
    let origin: Arc<Mutex<(f32, f32)>> = Arc::new(Mutex::new((0.0, 0.0)));

    div()
        .id("diagram-page")
        .size_full()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(theme.background)
        .child(render_toolbar(&work, cx))
        .child(render_canvas(work.clone(), origin, cx))
        .child(render_status(&work, cx))
}

fn tool_button(id: &'static str, label: &str, active: bool, tool: DiagramTool) -> Button {
    Button::new(id)
        .variant(if active {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Ghost
        })
        .size(ButtonSize::Small)
        .label(label.to_string())
        .on_click(move |_, _, cx| Bridge::diagram_set_tool(tool, cx))
}

fn render_toolbar(work: &DiagramWork, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let tool = work.active_tool;
    let shape_ids = [
        "dg-shape-process",
        "dg-shape-decision",
        "dg-shape-terminal",
        "dg-shape-db",
        "dg-shape-sub",
        "dg-shape-doc",
    ];
    div()
        .w_full()
        .flex_none()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_1()
        .px_2()
        .py_1p5()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.35))
        .child(tool_button(
            "dg-select",
            "Select",
            tool == DiagramTool::Select,
            DiagramTool::Select,
        ))
        .child(tool_button(
            "dg-connect",
            "Connect",
            tool == DiagramTool::Connect,
            DiagramTool::Connect,
        ))
        .child(div().w(px(1.)).h(px(18.)).bg(theme.border).mx_1())
        .children(DiagramShapeKind::ALL.iter().zip(shape_ids).map(|(k, id)| {
            let t = DiagramTool::AddShape(*k);
            tool_button(id, k.label(), tool == t, t)
        }))
        .child(tool_button(
            "dg-table",
            "Table",
            tool == DiagramTool::AddTable,
            DiagramTool::AddTable,
        ))
        .child(div().flex_1())
        .child(
            Button::new("dg-grid")
                .variant(if work.grid_snap {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Ghost
                })
                .size(ButtonSize::Small)
                .label("Snap")
                .on_click(|_, _, cx| Bridge::diagram_toggle_grid_snap(cx)),
        )
        .child(
            Button::new("dg-zoom-reset")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .label(format!("{:.0}%", work.camera.zoom * 100.0))
                .on_click(|_, _, cx| Bridge::diagram_zoom_reset(cx)),
        )
        .child(
            Button::new("dg-delete")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .label("Delete")
                .on_click(|_, _, cx| Bridge::diagram_delete_selected(cx)),
        )
        .child(
            Button::new("dg-demo")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .label("Reset")
                .on_click(|_, _, cx| Bridge::diagram_reset_demo(cx)),
        )
        .child(
            Button::new("dg-note")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Small)
                .label("Insert into Note")
                .on_click(|_, _, cx| Bridge::diagram_insert_into_active_note(cx)),
        )
}

fn render_status(work: &DiagramWork, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let mut bar = div()
        .w_full()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .border_t_1()
        .border_color(theme.border)
        .text_xs()
        .text_color(theme.muted_foreground);

    let hint = match work.active_tool {
        DiagramTool::Select => {
            "Drag to move · drag empty space to pan · scroll to pan · ⌘/Ctrl+scroll to zoom"
                .to_string()
        }
        DiagramTool::Pan => "Drag to pan".to_string(),
        DiagramTool::AddShape(k) => format!("Click the canvas to place a {}", k.label()),
        DiagramTool::AddTable => "Click the canvas to place a table".to_string(),
        DiagramTool::Connect => match &work.connecting_from {
            Some(_) => "Click the target node".to_string(),
            None => "Click the source node".to_string(),
        },
    };
    bar = bar.child(hint).child(div().flex_1());

    if let Some(tid) = work.selected_table_id.clone() {
        let (a, b, c) = (tid.clone(), tid.clone(), tid);
        bar = bar
            .child(
                Button::new("dg-row-add")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .label("+ Row")
                    .on_click(move |_, _, cx| Bridge::diagram_table_add_row(&a, cx)),
            )
            .child(
                Button::new("dg-row-del")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .label("− Row")
                    .on_click(move |_, _, cx| Bridge::diagram_table_remove_row(&b, cx)),
            )
            .child(
                Button::new("dg-col-add")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .label("+ Column")
                    .on_click(move |_, _, cx| Bridge::diagram_table_add_col(&c, cx)),
            );
    }
    if let Some(eid) = work.selected_edge_id.clone() {
        for (i, route) in DiagramEdgeRoute::ALL.iter().enumerate() {
            let eid = eid.clone();
            let route = *route;
            let id: SharedString = format!("dg-route-{i}").into();
            bar = bar.child(
                Button::new(id)
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .label(route.label())
                    .on_click(move |_, _, cx| Bridge::diagram_update_edge_route(&eid, route, cx)),
            );
        }
    }
    bar.child(format!(
        "{} nodes · {} edges · {} tables",
        work.nodes.len(),
        work.edges.len(),
        work.tables.len()
    ))
}

fn render_canvas(work: DiagramWork, origin: Arc<Mutex<(f32, f32)>>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let cam = work.camera;
    let z = cam.zoom;
    let grid_color = theme.border.opacity(0.35);

    // Shared with the paint closure so handlers can convert window → canvas coords.
    let paint_origin = origin.clone();
    let paint_work = work.clone();

    let surface = canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            *paint_origin.lock().unwrap_or_else(|e| e.into_inner()) =
                (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
            paint_scene(window, bounds, &paint_work, grid_color);
        },
    )
    .size_full();

    // Text + tables as overlay children, positioned in screen space.
    let mut overlay = div().absolute().inset_0().overflow_hidden();
    for n in &work.nodes {
        let (sx, sy) = cam.world_to_screen(n.x, n.y);
        overlay = overlay.child(
            div()
                .absolute()
                .left(px(sx))
                .top(px(sy))
                .w(px(n.width * z))
                .h(px(n.height * z))
                .flex()
                .items_center()
                .justify_center()
                .px(px(8.0 * z))
                .text_center()
                .text_size(px(n.font_size * z))
                .text_color(rgb(n.text_color))
                .child(n.label.clone()),
        );
    }
    for e in &work.edges {
        let (Some(label), Some(from), Some(to)) = (
            e.label.as_ref(),
            work.nodes.iter().find(|n| n.id == e.from_node),
            work.nodes.iter().find(|n| n.id == e.to_node),
        ) else {
            continue;
        };
        let (path, _) = router::route_edge(e, from, to);
        let (mx, my) = router::path_midpoint(&path);
        let (sx, sy) = cam.world_to_screen(mx, my);
        overlay = overlay.child(
            div()
                .absolute()
                .left(px(sx - 20.0 * z))
                .top(px(sy - 9.0 * z))
                .px(px(4.0 * z))
                .rounded(px(3.0 * z))
                .bg(theme.background.opacity(0.9))
                .text_size(px(11.0 * z))
                .text_color(rgb(e.color))
                .child(label.clone()),
        );
    }
    for t in &work.tables {
        let (sx, sy) = cam.world_to_screen(t.x, t.y);
        overlay = overlay.child(table::render_table(
            t,
            sx,
            sy,
            z,
            work.selected_table_id.as_deref() == Some(t.id.as_str()),
        ));
    }

    let down_origin = origin.clone();
    let move_origin = origin.clone();
    let down_work = work.clone();
    let move_work = work;

    div()
        .id("diagram-canvas")
        .relative()
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_hidden()
        .child(surface)
        .child(overlay.id("diagram-overlay"))
        .on_mouse_down(MouseButton::Left, move |ev, _, cx| {
            let (ox, oy) = *down_origin.lock().unwrap_or_else(|e| e.into_inner());
            let (sx, sy) = (f32::from(ev.position.x) - ox, f32::from(ev.position.y) - oy);
            let (wx, wy) = down_work.camera.screen_to_world(sx, sy);
            let hit = hit_test(&down_work, wx, wy);
            match down_work.active_tool {
                DiagramTool::AddShape(kind) => {
                    Bridge::diagram_add_node(
                        kind,
                        snap(wx - 70.0, down_work.grid_snap),
                        snap(wy - 28.0, down_work.grid_snap),
                        cx,
                    );
                }
                DiagramTool::AddTable => {
                    Bridge::diagram_add_table(
                        snap(wx, down_work.grid_snap),
                        snap(wy, down_work.grid_snap),
                        cx,
                    );
                }
                DiagramTool::Connect => {
                    if let Some(Hit::Node(i)) = hit {
                        let node = &down_work.nodes[i];
                        match &down_work.connecting_from {
                            None => Bridge::diagram_start_connect(&node.id, Port::East, cx),
                            Some((from_id, _)) => {
                                if let Some(from) =
                                    down_work.nodes.iter().find(|n| &n.id == from_id)
                                {
                                    let fc =
                                        (from.x + from.width * 0.5, from.y + from.height * 0.5);
                                    let tc =
                                        (node.x + node.width * 0.5, node.y + node.height * 0.5);
                                    let (p0, p1) = auto_ports(fc, tc);
                                    Bridge::diagram_connect(from_id, p0, &node.id, p1, cx);
                                }
                            }
                        }
                    } else {
                        Bridge::diagram_cancel_connect(cx);
                    }
                }
                DiagramTool::Select | DiagramTool::Pan => match hit {
                    Some(Hit::Node(i)) if down_work.active_tool == DiagramTool::Select => {
                        let n = &down_work.nodes[i];
                        *DRAG_OFFSET.lock().unwrap_or_else(|e| e.into_inner()) =
                            (wx - n.x, wy - n.y);
                        Bridge::diagram_select_node(Some(n.id.clone()), cx);
                    }
                    Some(Hit::Table(i)) if down_work.active_tool == DiagramTool::Select => {
                        let t = &down_work.tables[i];
                        *DRAG_OFFSET.lock().unwrap_or_else(|e| e.into_inner()) =
                            (wx - t.x, wy - t.y);
                        Bridge::diagram_select_table(Some(t.id.clone()), cx);
                    }
                    _ => {
                        Bridge::diagram_select_node(None, cx);
                        *PAN_LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some((sx, sy));
                    }
                },
            }
        })
        .on_mouse_move(move |ev, _, cx| {
            if ev.pressed_button != Some(MouseButton::Left) {
                return;
            }
            let (ox, oy) = *move_origin.lock().unwrap_or_else(|e| e.into_inner());
            let (sx, sy) = (f32::from(ev.position.x) - ox, f32::from(ev.position.y) - oy);

            let mut pan = PAN_LAST.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((lx, ly)) = *pan {
                Bridge::diagram_pan(sx - lx, sy - ly, cx);
                *pan = Some((sx, sy));
                return;
            }
            drop(pan);

            if move_work.active_tool != DiagramTool::Select {
                return;
            }
            let (wx, wy) = move_work.camera.screen_to_world(sx, sy);
            let (dx, dy) = *DRAG_OFFSET.lock().unwrap_or_else(|e| e.into_inner());
            let (nx, ny) = (
                snap(wx - dx, move_work.grid_snap),
                snap(wy - dy, move_work.grid_snap),
            );
            if let Some(id) = &move_work.selected_node_id {
                Bridge::diagram_move_node(id, nx, ny, cx);
            } else if let Some(id) = &move_work.selected_table_id {
                Bridge::diagram_move_table(id, nx, ny, cx);
            }
        })
        .on_mouse_up(MouseButton::Left, |_, _, _| {
            *PAN_LAST.lock().unwrap_or_else(|e| e.into_inner()) = None;
        })
        .on_mouse_up_out(MouseButton::Left, |_, _, _| {
            *PAN_LAST.lock().unwrap_or_else(|e| e.into_inner()) = None;
        })
        .on_scroll_wheel({
            let scroll_origin = origin;
            move |ev, _, cx| {
                let d = ev.delta.pixel_delta(px(1.0));
                if ev.modifiers.platform || ev.modifiers.control {
                    let (ox, oy) = *scroll_origin.lock().unwrap_or_else(|e| e.into_inner());
                    let (sx, sy) = (f32::from(ev.position.x) - ox, f32::from(ev.position.y) - oy);
                    Bridge::diagram_zoom(f32::from(d.y) * 0.004, sx, sy, cx);
                } else {
                    Bridge::diagram_pan(f32::from(d.x), f32::from(d.y), cx);
                }
            }
        })
}

fn paint_scene(window: &mut Window, bounds: Bounds<Pixels>, work: &DiagramWork, grid_color: Hsla) {
    let cam: Camera2D = work.camera;
    let ox = bounds.origin.x;
    let oy = bounds.origin.y;
    let to_screen = |wx: f32, wy: f32| {
        let (sx, sy) = cam.world_to_screen(wx, wy);
        point(ox + px(sx), oy + px(sy))
    };

    // Dot grid.
    let step = GRID * cam.zoom;
    if step >= 6.0 {
        let w = f32::from(bounds.size.width);
        let h = f32::from(bounds.size.height);
        let start_x = cam.offset_x.rem_euclid(step);
        let start_y = cam.offset_y.rem_euclid(step);
        let mut y = start_y;
        while y < h {
            let mut x = start_x;
            while x < w {
                window.paint_quad(fill(
                    Bounds {
                        origin: point(ox + px(x), oy + px(y)),
                        size: size(px(1.5), px(1.5)),
                    },
                    grid_color,
                ));
                x += step;
            }
            y += step;
        }
    }

    // Edges first so nodes draw on top.
    for e in &work.edges {
        let (Some(from), Some(to)) = (
            work.nodes.iter().find(|n| n.id == e.from_node),
            work.nodes.iter().find(|n| n.id == e.to_node),
        ) else {
            continue;
        };
        let selected = work.selected_edge_id.as_deref() == Some(e.id.as_str());
        let color = if selected {
            rgb(0x38bdf8)
        } else {
            rgb(e.color)
        };
        let (path, dir) = router::route_edge(e, from, to);
        let mut b = PathBuilder::stroke(px(e.stroke_width * cam.zoom));
        let tip;
        match &path {
            router::RoutedPath::Polyline(pts) => {
                b.move_to(to_screen(pts[0].0, pts[0].1));
                for p in &pts[1..] {
                    b.line_to(to_screen(p.0, p.1));
                }
                let last = pts[pts.len() - 1];
                tip = to_screen(last.0, last.1);
            }
            router::RoutedPath::Cubic { from, c1, c2, to } => {
                b.move_to(to_screen(from.0, from.1));
                b.cubic_bezier_to(
                    to_screen(to.0, to.1),
                    to_screen(c1.0, c1.1),
                    to_screen(c2.0, c2.1),
                );
                tip = to_screen(to.0, to.1);
            }
        }
        if let Ok(p) = b.build() {
            window.paint_path(p, color);
        }
        if e.has_arrow {
            shapes::paint_arrowhead(
                window,
                tip,
                dir.0,
                dir.1,
                11.0 * cam.zoom,
                5.0 * cam.zoom,
                color,
            );
        }
    }

    for n in &work.nodes {
        shapes::paint_node_shape(
            window,
            to_screen(n.x, n.y),
            size(px(n.width * cam.zoom), px(n.height * cam.zoom)),
            n.kind,
            rgb(n.fill_color),
            rgb(n.stroke_color),
            px(n.stroke_width * cam.zoom),
            work.selected_node_id.as_deref() == Some(n.id.as_str()),
        );
    }
}
