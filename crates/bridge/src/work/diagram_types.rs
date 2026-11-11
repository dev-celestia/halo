//! Data structures for the 2D Diagram, Flowchart, and Table engine.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DiagramShapeKind {
    #[default]
    Process,     // Standard rectangle
    Decision,    // Diamond
    Terminal,    // Pill / stadium shape
    Database,    // Cylinder
    Subroutine,  // Process with double vertical edges
    Document,    // Document with curved bottom
}

impl DiagramShapeKind {
    pub const ALL: [Self; 6] = [
        Self::Process,
        Self::Decision,
        Self::Terminal,
        Self::Database,
        Self::Subroutine,
        Self::Document,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Process => "Process",
            Self::Decision => "Decision",
            Self::Terminal => "Terminal / Start-End",
            Self::Database => "Database",
            Self::Subroutine => "Subroutine",
            Self::Document => "Document",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Port {
    #[default]
    North,
    South,
    East,
    West,
}

impl Port {
    pub const ALL: [Self; 4] = [Self::North, Self::South, Self::East, Self::West];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiagramNode {
    pub id: String,
    pub label: String,
    pub kind: DiagramShapeKind,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub fill_color: u32,
    pub stroke_color: u32,
    pub stroke_width: f32,
    pub text_color: u32,
    pub font_size: f32,
}

impl Eq for DiagramNode {}

impl Default for DiagramNode {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: "Node".to_string(),
            kind: DiagramShapeKind::Process,
            x: 100.0,
            y: 100.0,
            width: 140.0,
            height: 60.0,
            fill_color: 0x1e293b,
            stroke_color: 0x38bdf8,
            stroke_width: 2.0,
            text_color: 0xf8fafc,
            font_size: 13.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DiagramEdgeRoute {
    #[default]
    CubicBezier,
    Orthogonal,
    Straight,
}

impl DiagramEdgeRoute {
    pub const ALL: [Self; 3] = [Self::CubicBezier, Self::Orthogonal, Self::Straight];

    pub fn label(self) -> &'static str {
        match self {
            Self::CubicBezier => "Bézier Curve",
            Self::Orthogonal => "Orthogonal (90°)",
            Self::Straight => "Straight Line",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiagramEdge {
    pub id: String,
    pub from_node: String,
    pub from_port: Port,
    pub to_node: String,
    pub to_port: Port,
    pub label: Option<String>,
    pub route: DiagramEdgeRoute,
    pub color: u32,
    pub stroke_width: f32,
    pub has_arrow: bool,
}

impl Eq for DiagramEdge {}

impl Default for DiagramEdge {
    fn default() -> Self {
        Self {
            id: String::new(),
            from_node: String::new(),
            from_port: Port::East,
            to_node: String::new(),
            to_port: Port::West,
            label: None,
            route: DiagramEdgeRoute::CubicBezier,
            color: 0x94a3b8,
            stroke_width: 2.0,
            has_arrow: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiagramTable {
    pub id: String,
    pub title: String,
    pub x: f32,
    pub y: f32,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub col_widths: Vec<f32>,
    pub header_bg: u32,
    pub row_stripe_bg: u32,
    pub border_color: u32,
}

impl Eq for DiagramTable {}

impl Default for DiagramTable {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: "Data Table".to_string(),
            x: 200.0,
            y: 300.0,
            headers: vec!["ID".into(), "Name".into(), "Status".into(), "Type".into()],
            rows: vec![
                vec!["1".into(), "auth_service".into(), "Active".into(), "Core".into()],
                vec!["2".into(), "api_gateway".into(), "Active".into(), "Ingress".into()],
                vec!["3".into(), "cache_cluster".into(), "Standby".into(), "Infra".into()],
            ],
            col_widths: vec![50.0, 140.0, 90.0, 80.0],
            header_bg: 0x0f172a,
            row_stripe_bg: 0x1e293b,
            border_color: 0x334155,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DiagramTool {
    #[default]
    Select,
    Pan,
    AddShape(DiagramShapeKind),
    AddTable,
    Connect,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera2D {
    pub offset_x: f32,
    pub offset_y: f32,
    pub zoom: f32,
}

impl Eq for Camera2D {}

impl Default for Camera2D {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            zoom: 1.0,
        }
    }
}

impl Camera2D {
    pub fn screen_to_world(&self, sx: f32, sy: f32) -> (f32, f32) {
        (
            (sx - self.offset_x) / self.zoom,
            (sy - self.offset_y) / self.zoom,
        )
    }

    pub fn world_to_screen(&self, wx: f32, wy: f32) -> (f32, f32) {
        (
            wx * self.zoom + self.offset_x,
            wy * self.zoom + self.offset_y,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiagramWork {
    pub nodes: Vec<DiagramNode>,
    pub edges: Vec<DiagramEdge>,
    pub tables: Vec<DiagramTable>,
    pub selected_node_id: Option<String>,
    pub selected_table_id: Option<String>,
    pub selected_edge_id: Option<String>,
    pub connecting_from: Option<(String, Port)>,
    pub camera: Camera2D,
    pub active_tool: DiagramTool,
    pub grid_snap: bool,
    pub history_can_undo: bool,
    pub history_can_redo: bool,
}

impl Eq for DiagramWork {}

impl Default for DiagramWork {
    fn default() -> Self {
        let mut work = Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            tables: Vec::new(),
            selected_node_id: None,
            selected_table_id: None,
            selected_edge_id: None,
            connecting_from: None,
            camera: Camera2D::default(),
            active_tool: DiagramTool::Select,
            grid_snap: true,
            history_can_undo: false,
            history_can_redo: false,
        };
        work.load_demo();
        work
    }
}

impl DiagramWork {
    /// Loads a clean starter flowchart and architecture table.
    pub fn load_demo(&mut self) {
        self.nodes = vec![
            DiagramNode {
                id: "node_start".into(),
                label: "Client Ingress".into(),
                kind: DiagramShapeKind::Terminal,
                x: 80.0,
                y: 120.0,
                width: 140.0,
                height: 54.0,
                fill_color: 0x064e3b,   // Emerald dark
                stroke_color: 0x34d399, // Emerald bright
                stroke_width: 2.0,
                text_color: 0xf0fdf4,
                font_size: 13.0,
            },
            DiagramNode {
                id: "node_decision".into(),
                label: "Inspect Proxy\nPolicy?".into(),
                kind: DiagramShapeKind::Decision,
                x: 290.0,
                y: 100.0,
                width: 130.0,
                height: 94.0,
                fill_color: 0x451a03,   // Amber dark
                stroke_color: 0xfbbf24, // Amber bright
                stroke_width: 2.0,
                text_color: 0xfefce8,
                font_size: 12.0,
            },
            DiagramNode {
                id: "node_pass".into(),
                label: "Forward to\nUpstream Service".into(),
                kind: DiagramShapeKind::Process,
                x: 500.0,
                y: 60.0,
                width: 150.0,
                height: 56.0,
                fill_color: 0x1e3a5f,   // Blue dark
                stroke_color: 0x60a5fa, // Blue bright
                stroke_width: 2.0,
                text_color: 0xeff6ff,
                font_size: 12.0,
            },
            DiagramNode {
                id: "node_intercept".into(),
                label: "Intercept & Trap\nRule Engine".into(),
                kind: DiagramShapeKind::Process,
                x: 500.0,
                y: 180.0,
                width: 150.0,
                height: 56.0,
                fill_color: 0x4c1d95,   // Purple dark
                stroke_color: 0xa78bfa, // Purple bright
                stroke_width: 2.0,
                text_color: 0xf5f3ff,
                font_size: 12.0,
            },
            DiagramNode {
                id: "node_db".into(),
                label: "SQLite History\nAudit Log".into(),
                kind: DiagramShapeKind::Database,
                x: 720.0,
                y: 120.0,
                width: 130.0,
                height: 64.0,
                fill_color: 0x1f2937,   // Slate
                stroke_color: 0x94a3b8, // Light slate
                stroke_width: 2.0,
                text_color: 0xf8fafc,
                font_size: 12.0,
            },
        ];

        self.edges = vec![
            DiagramEdge {
                id: "edge_1".into(),
                from_node: "node_start".into(),
                from_port: Port::East,
                to_node: "node_decision".into(),
                to_port: Port::West,
                label: None,
                route: DiagramEdgeRoute::CubicBezier,
                color: 0x38bdf8,
                stroke_width: 2.0,
                has_arrow: true,
            },
            DiagramEdge {
                id: "edge_2".into(),
                from_node: "node_decision".into(),
                from_port: Port::North,
                to_node: "node_pass".into(),
                to_port: Port::West,
                label: Some("Allow".into()),
                route: DiagramEdgeRoute::Orthogonal,
                color: 0x34d399,
                stroke_width: 2.0,
                has_arrow: true,
            },
            DiagramEdge {
                id: "edge_3".into(),
                from_node: "node_decision".into(),
                from_port: Port::South,
                to_node: "node_intercept".into(),
                to_port: Port::West,
                label: Some("Trap".into()),
                route: DiagramEdgeRoute::Orthogonal,
                color: 0xf87171,
                stroke_width: 2.0,
                has_arrow: true,
            },
            DiagramEdge {
                id: "edge_4".into(),
                from_node: "node_pass".into(),
                from_port: Port::East,
                to_node: "node_db".into(),
                to_port: Port::West,
                label: None,
                route: DiagramEdgeRoute::CubicBezier,
                color: 0x94a3b8,
                stroke_width: 2.0,
                has_arrow: true,
            },
            DiagramEdge {
                id: "edge_5".into(),
                from_node: "node_intercept".into(),
                from_port: Port::East,
                to_node: "node_db".into(),
                to_port: Port::West,
                label: None,
                route: DiagramEdgeRoute::CubicBezier,
                color: 0xa78bfa,
                stroke_width: 2.0,
                has_arrow: true,
            },
        ];

        self.tables = vec![DiagramTable {
            id: "table_routes".into(),
            title: "Microservices Routing Manifest".into(),
            x: 80.0,
            y: 280.0,
            headers: vec![
                "Service Name".into(),
                "Endpoint".into(),
                "Protocol".into(),
                "Auth Level".into(),
            ],
            rows: vec![
                vec![
                    "auth-provider".into(),
                    "127.0.0.1:8080".into(),
                    "HTTPS/mTLS".into(),
                    "OAuth2/Bearer".into(),
                ],
                vec![
                    "proxy-pipeline".into(),
                    "127.0.0.1:8443".into(),
                    "HTTP/2 Intercept".into(),
                    "Internal Root".into(),
                ],
                vec![
                    "storage-cluster".into(),
                    "127.0.0.1:9000".into(),
                    "S3 / R2 API".into(),
                    "HMAC-SHA256".into(),
                ],
            ],
            col_widths: vec![130.0, 130.0, 130.0, 120.0],
            header_bg: 0x0f172a,
            row_stripe_bg: 0x1e293b,
            border_color: 0x334155,
        }];
    }
}
