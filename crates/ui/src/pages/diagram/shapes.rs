//! 2D Vector Shape & Path Builder primitives (Lyon-backed GPUI `PathBuilder`).
//!
//! Constructs filled and stroked vector polygons and curves for flowchart nodes:
//! - Process (Rectangle)
//! - Decision (Diamond)
//! - Terminal (Pill / Stadium with semicircular caps)
//! - Database (Cylinder with elliptical top/bottom curves)
//! - Subroutine (Double-bordered process box)
//! - Document (Rectangle with cubic Bézier wavy bottom)

use gpui::*;
use halo_bridge::{DiagramNode, DiagramShapeKind, Port};

/// Compute the world-space `(x, y)` coordinates of a node's connection port.
pub fn node_port_world(node: &DiagramNode, port: Port) -> (f32, f32) {
    match port {
        Port::North => (node.x + node.width * 0.5, node.y),
        Port::South => (node.x + node.width * 0.5, node.y + node.height),
        Port::East => (node.x + node.width, node.y + node.height * 0.5),
        Port::West => (node.x, node.y + node.height * 0.5),
    }
}

/// Outward unit normal vector `(nx, ny)` for a port.
pub fn port_normal(port: Port) -> (f32, f32) {
    match port {
        Port::North => (0.0, -1.0),
        Port::South => (0.0, 1.0),
        Port::East => (1.0, 0.0),
        Port::West => (-1.0, 0.0),
    }
}

/// Paint a flowchart node's vector geometry (fill + stroke + inner details) onto the GPUI window.
pub fn paint_node_shape(
    window: &mut Window,
    origin: Point<Pixels>,
    size: Size<Pixels>,
    kind: DiagramShapeKind,
    fill_color: Rgba,
    stroke_color: Rgba,
    stroke_width: Pixels,
    is_selected: bool,
) {
    let x = origin.x;
    let y = origin.y;
    let w = size.width;
    let h = size.height;

    match kind {
        DiagramShapeKind::Process => {
            paint_rect_path(window, x, y, w, h, fill_color, stroke_color, stroke_width);
        }
        DiagramShapeKind::Decision => {
            paint_diamond_path(window, x, y, w, h, fill_color, stroke_color, stroke_width);
        }
        DiagramShapeKind::Terminal => {
            paint_pill_path(window, x, y, w, h, fill_color, stroke_color, stroke_width);
        }
        DiagramShapeKind::Database => {
            paint_cylinder_path(window, x, y, w, h, fill_color, stroke_color, stroke_width);
        }
        DiagramShapeKind::Subroutine => {
            paint_rect_path(window, x, y, w, h, fill_color, stroke_color, stroke_width);
            let inset = px(10.0).min(w * 0.12);
            let mut inner = PathBuilder::stroke(stroke_width);
            inner.move_to(point(x + inset, y));
            inner.line_to(point(x + inset, y + h));
            inner.move_to(point(x + w - inset, y));
            inner.line_to(point(x + w - inset, y + h));
            if let Ok(path) = inner.build() {
                window.paint_path(path, stroke_color);
            }
        }
        DiagramShapeKind::Document => {
            paint_document_path(window, x, y, w, h, fill_color, stroke_color, stroke_width);
        }
    }

    if is_selected {
        let pad = px(4.0);
        let mut sel = PathBuilder::stroke(px(1.5));
        sel.move_to(point(x - pad, y - pad));
        sel.line_to(point(x + w + pad, y - pad));
        sel.line_to(point(x + w + pad, y + h + pad));
        sel.line_to(point(x - pad, y + h + pad));
        sel.close();
        if let Ok(path) = sel.build() {
            window.paint_path(path, rgb(0x38bdf8));
        }
    }
}

fn paint_rect_path(
    window: &mut Window,
    x: Pixels,
    y: Pixels,
    w: Pixels,
    h: Pixels,
    fill_color: Rgba,
    stroke_color: Rgba,
    stroke_width: Pixels,
) {
    let build = |mut b: PathBuilder| -> Option<Path<Pixels>> {
        b.move_to(point(x, y));
        b.line_to(point(x + w, y));
        b.line_to(point(x + w, y + h));
        b.line_to(point(x, y + h));
        b.close();
        b.build().ok()
    };
    if let Some(fill_path) = build(PathBuilder::fill()) {
        window.paint_path(fill_path, fill_color);
    }
    if let Some(stroke_path) = build(PathBuilder::stroke(stroke_width)) {
        window.paint_path(stroke_path, stroke_color);
    }
}

fn paint_diamond_path(
    window: &mut Window,
    x: Pixels,
    y: Pixels,
    w: Pixels,
    h: Pixels,
    fill_color: Rgba,
    stroke_color: Rgba,
    stroke_width: Pixels,
) {
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let build = |mut b: PathBuilder| -> Option<Path<Pixels>> {
        b.move_to(point(cx, y));
        b.line_to(point(x + w, cy));
        b.line_to(point(cx, y + h));
        b.line_to(point(x, cy));
        b.close();
        b.build().ok()
    };
    if let Some(fill_path) = build(PathBuilder::fill()) {
        window.paint_path(fill_path, fill_color);
    }
    if let Some(stroke_path) = build(PathBuilder::stroke(stroke_width)) {
        window.paint_path(stroke_path, stroke_color);
    }
}

fn paint_pill_path(
    window: &mut Window,
    x: Pixels,
    y: Pixels,
    w: Pixels,
    h: Pixels,
    fill_color: Rgba,
    stroke_color: Rgba,
    stroke_width: Pixels,
) {
    let r = (h * 0.5).min(w * 0.5);
    let k = r * 0.55228475; // cubic Bézier circle approximation constant
    let build = |mut b: PathBuilder| -> Option<Path<Pixels>> {
        b.move_to(point(x + r, y));
        b.line_to(point(x + w - r, y));
        // Right cap top-to-middle
        b.cubic_bezier_to(
            point(x + w, y + r),
            point(x + w - r + k, y),
            point(x + w, y + r - k),
        );
        // Right cap middle-to-bottom
        b.cubic_bezier_to(
            point(x + w - r, y + h),
            point(x + w, y + r + k),
            point(x + w - r + k, y + h),
        );
        b.line_to(point(x + r, y + h));
        // Left cap bottom-to-middle
        b.cubic_bezier_to(
            point(x, y + r),
            point(x + r - k, y + h),
            point(x, y + r + k),
        );
        // Left cap middle-to-top
        b.cubic_bezier_to(point(x + r, y), point(x, y + r - k), point(x + r - k, y));
        b.close();
        b.build().ok()
    };
    if let Some(fill_path) = build(PathBuilder::fill()) {
        window.paint_path(fill_path, fill_color);
    }
    if let Some(stroke_path) = build(PathBuilder::stroke(stroke_width)) {
        window.paint_path(stroke_path, stroke_color);
    }
}

fn paint_cylinder_path(
    window: &mut Window,
    x: Pixels,
    y: Pixels,
    w: Pixels,
    h: Pixels,
    fill_color: Rgba,
    stroke_color: Rgba,
    stroke_width: Pixels,
) {
    let cap_h = (h * 0.22).min(px(14.0));
    let build_outer = |mut b: PathBuilder| -> Option<Path<Pixels>> {
        b.move_to(point(x, y + cap_h * 0.5));
        // Top curve
        b.cubic_bezier_to(
            point(x + w, y + cap_h * 0.5),
            point(x + w * 0.25, y - cap_h * 0.35),
            point(x + w * 0.75, y - cap_h * 0.35),
        );
        b.line_to(point(x + w, y + h - cap_h * 0.5));
        // Bottom curve
        b.cubic_bezier_to(
            point(x, y + h - cap_h * 0.5),
            point(x + w * 0.75, y + h + cap_h * 0.35),
            point(x + w * 0.25, y + h + cap_h * 0.35),
        );
        b.close();
        b.build().ok()
    };
    if let Some(fill_path) = build_outer(PathBuilder::fill()) {
        window.paint_path(fill_path, fill_color);
    }
    if let Some(stroke_path) = build_outer(PathBuilder::stroke(stroke_width)) {
        window.paint_path(stroke_path, stroke_color);
    }
    // Front rim curve of top cap
    let mut rim = PathBuilder::stroke(stroke_width);
    rim.move_to(point(x, y + cap_h * 0.5));
    rim.cubic_bezier_to(
        point(x + w, y + cap_h * 0.5),
        point(x + w * 0.25, y + cap_h * 1.35),
        point(x + w * 0.75, y + cap_h * 1.35),
    );
    if let Ok(path) = rim.build() {
        window.paint_path(path, stroke_color);
    }
}

fn paint_document_path(
    window: &mut Window,
    x: Pixels,
    y: Pixels,
    w: Pixels,
    h: Pixels,
    fill_color: Rgba,
    stroke_color: Rgba,
    stroke_width: Pixels,
) {
    let wave = (h * 0.14).min(px(10.0));
    let build = |mut b: PathBuilder| -> Option<Path<Pixels>> {
        b.move_to(point(x, y));
        b.line_to(point(x + w, y));
        b.line_to(point(x + w, y + h - wave));
        b.cubic_bezier_to(
            point(x, y + h - wave * 0.5),
            point(x + w * 0.66, y + h - wave * 2.2),
            point(x + w * 0.33, y + h + wave * 1.2),
        );
        b.close();
        b.build().ok()
    };
    if let Some(fill_path) = build(PathBuilder::fill()) {
        window.paint_path(fill_path, fill_color);
    }
    if let Some(stroke_path) = build(PathBuilder::stroke(stroke_width)) {
        window.paint_path(stroke_path, stroke_color);
    }
}

/// Paint a filled triangle arrowhead at `tip` pointing along unit vector `(dx, dy)`.
pub fn paint_arrowhead(
    window: &mut Window,
    tip: Point<Pixels>,
    dx: f32,
    dy: f32,
    length: f32,
    half_width: f32,
    color: Rgba,
) {
    // Base center of the arrowhead
    let bx = tip.x - px(dx * length);
    let by = tip.y - px(dy * length);
    // Perpendicular vector (-dy, dx)
    let px_vec = -dy * half_width;
    let py_vec = dx * half_width;

    let left = point(bx + px(px_vec), by + px(py_vec));
    let right = point(bx - px(px_vec), by - px(py_vec));

    let mut b = PathBuilder::fill();
    b.move_to(tip);
    b.line_to(left);
    b.line_to(right);
    b.close();
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}
