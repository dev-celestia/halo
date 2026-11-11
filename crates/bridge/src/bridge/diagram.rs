//! Bridge commands for the 2D Diagram, Flowchart, and Table Studio.

use gpui::App;

use crate::work::{Camera2D, DiagramEdge, DiagramNode, DiagramShapeKind, DiagramTable, DiagramTool, Port};
use crate::Bridge;

impl Bridge {
    pub fn diagram_set_tool(tool: DiagramTool, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.diagram.active_tool = tool;
            s.diagram.connecting_from = None;
        });
    }

    pub fn diagram_set_camera(camera: Camera2D, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.diagram.camera = camera;
        });
    }

    pub fn diagram_pan(dx: f32, dy: f32, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.diagram.camera.offset_x += dx;
            s.diagram.camera.offset_y += dy;
        });
    }

    pub fn diagram_zoom(delta: f32, cursor_x: f32, cursor_y: f32, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            let old_zoom = s.diagram.camera.zoom;
            let new_zoom = (old_zoom + delta).clamp(0.2, 3.0);
            if (new_zoom - old_zoom).abs() < f32::EPSILON {
                return;
            }
            // Zoom towards cursor
            let wx = (cursor_x - s.diagram.camera.offset_x) / old_zoom;
            let wy = (cursor_y - s.diagram.camera.offset_y) / old_zoom;
            s.diagram.camera.zoom = new_zoom;
            s.diagram.camera.offset_x = cursor_x - wx * new_zoom;
            s.diagram.camera.offset_y = cursor_y - wy * new_zoom;
        });
    }

    pub fn diagram_zoom_reset(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.diagram.camera = Camera2D {
                offset_x: 0.0,
                offset_y: 0.0,
                zoom: 1.0,
            };
        });
    }

    pub fn diagram_select_node(id: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.diagram.selected_node_id = id;
            s.diagram.selected_table_id = None;
            s.diagram.selected_edge_id = None;
        });
    }

    pub fn diagram_select_table(id: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.diagram.selected_table_id = id;
            s.diagram.selected_node_id = None;
            s.diagram.selected_edge_id = None;
        });
    }

    pub fn diagram_select_edge(id: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.diagram.selected_edge_id = id;
            s.diagram.selected_node_id = None;
            s.diagram.selected_table_id = None;
        });
    }

    pub fn diagram_add_node(kind: DiagramShapeKind, x: f32, y: f32, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            let id = format!("node_{}", uuid::Uuid::new_v4().simple());
            let (w, h, fill, stroke) = match kind {
                DiagramShapeKind::Process => (140.0, 56.0, 0x1e293b, 0x38bdf8),
                DiagramShapeKind::Decision => (130.0, 90.0, 0x451a03, 0xfbbf24),
                DiagramShapeKind::Terminal => (140.0, 54.0, 0x064e3b, 0x34d399),
                DiagramShapeKind::Database => (130.0, 64.0, 0x1f2937, 0x94a3b8),
                DiagramShapeKind::Subroutine => (140.0, 56.0, 0x1e1b4b, 0x818cf8),
                DiagramShapeKind::Document => (140.0, 60.0, 0x312e81, 0xa5b4fc),
            };
            let node = DiagramNode {
                id: id.clone(),
                label: format!("New {}", kind.label()),
                kind,
                x,
                y,
                width: w,
                height: h,
                fill_color: fill,
                stroke_color: stroke,
                stroke_width: 2.0,
                text_color: 0xf8fafc,
                font_size: 13.0,
            };
            s.diagram.nodes.push(node);
            s.diagram.selected_node_id = Some(id);
            s.diagram.selected_table_id = None;
            s.diagram.active_tool = DiagramTool::Select;
        });
    }

    pub fn diagram_add_table(x: f32, y: f32, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            let id = format!("table_{}", uuid::Uuid::new_v4().simple());
            let table = DiagramTable {
                id: id.clone(),
                title: "New Table".into(),
                x,
                y,
                headers: vec!["Col 1".into(), "Col 2".into(), "Col 3".into()],
                rows: vec![
                    vec!["Data 1".into(), "Data 2".into(), "Data 3".into()],
                    vec!["Data 4".into(), "Data 5".into(), "Data 6".into()],
                ],
                col_widths: vec![120.0, 120.0, 120.0],
                header_bg: 0x0f172a,
                row_stripe_bg: 0x1e293b,
                border_color: 0x334155,
            };
            s.diagram.tables.push(table);
            s.diagram.selected_table_id = Some(id);
            s.diagram.selected_node_id = None;
            s.diagram.active_tool = DiagramTool::Select;
        });
    }

    pub fn diagram_move_node(id: &str, x: f32, y: f32, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if let Some(node) = s.diagram.nodes.iter_mut().find(|n| n.id == id) {
                node.x = x;
                node.y = y;
            }
        });
    }

    pub fn diagram_move_table(id: &str, x: f32, y: f32, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if let Some(table) = s.diagram.tables.iter_mut().find(|t| t.id == id) {
                table.x = x;
                table.y = y;
            }
        });
    }

    pub fn diagram_start_connect(from_node: &str, from_port: Port, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.diagram.connecting_from = Some((from_node.to_string(), from_port));
        });
    }

    pub fn diagram_cancel_connect(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.diagram.connecting_from = None;
        });
    }

    pub fn diagram_connect(from_node: &str, from_port: Port, to_node: &str, to_port: Port, cx: &mut App) {
        if from_node == to_node {
            return;
        }
        Self::state(cx).update(cx, |s| {
            let id = format!("edge_{}", uuid::Uuid::new_v4().simple());
            let edge = DiagramEdge {
                id: id.clone(),
                from_node: from_node.to_string(),
                from_port,
                to_node: to_node.to_string(),
                to_port,
                label: None,
                route: crate::work::DiagramEdgeRoute::CubicBezier,
                color: 0x38bdf8,
                stroke_width: 2.0,
                has_arrow: true,
            };
            s.diagram.edges.push(edge);
            s.diagram.connecting_from = None;
            s.diagram.selected_edge_id = Some(id);
        });
    }

    pub fn diagram_delete_selected(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if let Some(nid) = s.diagram.selected_node_id.take() {
                s.diagram.nodes.retain(|n| n.id != nid);
                s.diagram.edges.retain(|e| e.from_node != nid && e.to_node != nid);
            }
            if let Some(tid) = s.diagram.selected_table_id.take() {
                s.diagram.tables.retain(|t| t.id != tid);
            }
            if let Some(eid) = s.diagram.selected_edge_id.take() {
                s.diagram.edges.retain(|e| e.id != eid);
            }
        });
    }

    pub fn diagram_update_node_label(id: &str, label: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if let Some(node) = s.diagram.nodes.iter_mut().find(|n| n.id == id) {
                node.label = label.to_string();
            }
        });
    }

    pub fn diagram_update_node_color(id: &str, fill_color: u32, stroke_color: u32, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if let Some(node) = s.diagram.nodes.iter_mut().find(|n| n.id == id) {
                node.fill_color = fill_color;
                node.stroke_color = stroke_color;
            }
        });
    }

    pub fn diagram_update_node_shape(id: &str, kind: DiagramShapeKind, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if let Some(node) = s.diagram.nodes.iter_mut().find(|n| n.id == id) {
                node.kind = kind;
            }
        });
    }

    pub fn diagram_update_edge_route(id: &str, route: crate::work::DiagramEdgeRoute, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if let Some(edge) = s.diagram.edges.iter_mut().find(|e| e.id == id) {
                edge.route = route;
            }
        });
    }

    pub fn diagram_table_add_row(id: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if let Some(table) = s.diagram.tables.iter_mut().find(|t| t.id == id) {
                let cols = table.headers.len();
                let row_idx = table.rows.len() + 1;
                let new_row = (0..cols)
                    .map(|c| format!("R{row_idx}C{}", c + 1))
                    .collect();
                table.rows.push(new_row);
            }
        });
    }

    pub fn diagram_table_remove_row(id: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if let Some(table) = s.diagram.tables.iter_mut().find(|t| t.id == id) {
                if table.rows.len() > 1 {
                    table.rows.pop();
                }
            }
        });
    }

    pub fn diagram_table_add_col(id: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if let Some(table) = s.diagram.tables.iter_mut().find(|t| t.id == id) {
                let col_idx = table.headers.len() + 1;
                table.headers.push(format!("Col {col_idx}"));
                table.col_widths.push(110.0);
                for (r_idx, row) in table.rows.iter_mut().enumerate() {
                    row.push(format!("R{}C{col_idx}", r_idx + 1));
                }
            }
        });
    }

    pub fn diagram_toggle_grid_snap(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.diagram.grid_snap = !s.diagram.grid_snap;
        });
    }

    pub fn diagram_reset_demo(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.diagram.load_demo();
            s.diagram.selected_node_id = None;
            s.diagram.selected_table_id = None;
            s.diagram.selected_edge_id = None;
            s.diagram.connecting_from = None;
            s.diagram.camera = Camera2D::default();
        });
    }

    /// Generates a valid Mermaid flowchart string from the diagram state.
    pub fn diagram_to_mermaid(cx: &App) -> String {
        let state = Self::state(cx).read(cx);
        let mut mermaid = String::from("```mermaid\nflowchart TD\n");
        for node in &state.diagram.nodes {
            let clean_label = node.label.replace('\n', "<br/>");
            let shape = match node.kind {
                DiagramShapeKind::Process => format!("    {}[\"{}\"]\n", node.id, clean_label),
                DiagramShapeKind::Decision => format!("    {}{{\"{}\"}}\n", node.id, clean_label),
                DiagramShapeKind::Terminal => format!("    {}([\"{}\"])\n", node.id, clean_label),
                DiagramShapeKind::Database => format!("    {}[(\"{}\")]\n", node.id, clean_label),
                DiagramShapeKind::Subroutine => format!("    {}[[\"{}\"]]\n", node.id, clean_label),
                DiagramShapeKind::Document => format!("    {}>\"{}\"]\n", node.id, clean_label),
            };
            mermaid.push_str(&shape);
        }
        for edge in &state.diagram.edges {
            if let Some(label) = &edge.label {
                mermaid.push_str(&format!(
                    "    {} -->|\"{}\"| {}\n",
                    edge.from_node, label, edge.to_node
                ));
            } else {
                mermaid.push_str(&format!("    {} --> {}\n", edge.from_node, edge.to_node));
            }
        }
        mermaid.push_str("```\n");
        mermaid
    }

    /// Appends the current diagram as a Mermaid code block to the active Note.
    pub fn diagram_insert_into_active_note(cx: &mut App) {
        let mermaid = Self::diagram_to_mermaid(cx);
        let active_note_id = Self::state(cx).read(cx).notes_active_id.clone();
        if let Some(id) = active_note_id {
            let note_tuple = {
                let state = Self::state(cx).read(cx);
                state.notes.iter().find(|n| n.id == id).map(|n| (n.name.clone(), n.note.clone()))
            };
            if let Some((name, content)) = note_tuple {
                let updated = format!("{}\n\n{}", content.trim_end(), mermaid);
                let _ = Self::save_note(&id, &name, &updated, cx);
                Self::state(cx).update(cx, |s| {
                    s.apply_ok_msg("Inserted diagram into active Note".into());
                });
            }
        }
    }
}
