//! Edge routing: world-space polylines for straight, orthogonal, and Bézier connectors.

use halo_bridge::{DiagramEdge, DiagramEdgeRoute, DiagramNode};

use super::shapes::{node_port_world, port_normal};

/// A routed connector in world space.
pub enum RoutedPath {
    /// Straight segments through the listed points.
    Polyline(Vec<(f32, f32)>),
    /// Cubic Bézier `from → to` with two control points.
    Cubic {
        from: (f32, f32),
        c1: (f32, f32),
        c2: (f32, f32),
        to: (f32, f32),
    },
}

/// Route `edge` between its two nodes. Returns the path and the arrow direction at the tip.
pub fn route_edge(
    edge: &DiagramEdge,
    from: &DiagramNode,
    to: &DiagramNode,
) -> (RoutedPath, (f32, f32)) {
    let a = node_port_world(from, edge.from_port);
    let b = node_port_world(to, edge.to_port);
    let (n0x, n0y) = port_normal(edge.from_port);
    let (n1x, n1y) = port_normal(edge.to_port);
    // Arrow points *into* the target, i.e. opposite its outward normal.
    let dir = (-n1x, -n1y);

    match edge.route {
        DiagramEdgeRoute::Straight => (RoutedPath::Polyline(vec![a, b]), straight_dir(a, b)),
        DiagramEdgeRoute::CubicBezier => {
            let d = (((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt() * 0.4).clamp(30.0, 140.0);
            (
                RoutedPath::Cubic {
                    from: a,
                    c1: (a.0 + n0x * d, a.1 + n0y * d),
                    c2: (b.0 + n1x * d, b.1 + n1y * d),
                    to: b,
                },
                dir,
            )
        }
        DiagramEdgeRoute::Orthogonal => {
            const STUB: f32 = 24.0;
            let a2 = (a.0 + n0x * STUB, a.1 + n0y * STUB);
            let b2 = (b.0 + n1x * STUB, b.1 + n1y * STUB);
            let mut pts = vec![a, a2];
            if n0x != 0.0 {
                let mx = (a2.0 + b2.0) * 0.5;
                pts.push((mx, a2.1));
                pts.push((mx, b2.1));
            } else {
                let my = (a2.1 + b2.1) * 0.5;
                pts.push((a2.0, my));
                pts.push((b2.0, my));
            }
            pts.push(b2);
            pts.push(b);
            (RoutedPath::Polyline(pts), dir)
        }
    }
}

fn straight_dir(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    (dx / len, dy / len)
}

/// Midpoint of a routed path (for edge labels).
pub fn path_midpoint(path: &RoutedPath) -> (f32, f32) {
    match path {
        RoutedPath::Polyline(p) => {
            let m = p.len() / 2;
            if p.len() % 2 == 0 && m > 0 {
                ((p[m - 1].0 + p[m].0) * 0.5, (p[m - 1].1 + p[m].1) * 0.5)
            } else {
                p[m]
            }
        }
        RoutedPath::Cubic { from, c1, c2, to } => (
            (from.0 + 3.0 * c1.0 + 3.0 * c2.0 + to.0) / 8.0,
            (from.1 + 3.0 * c1.1 + 3.0 * c2.1 + to.1) / 8.0,
        ),
    }
}
