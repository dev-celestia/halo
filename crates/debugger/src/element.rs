//! Registered UI debug regions for the Elements inspector.
//!
//! GPUI has no React-style component props tree. Important shells opt in by
//! calling [`register_region`] (or the [`debug_region`] / [`region_bounds_sensor`]
//! helpers) during render so the debugger can list bounds / padding metadata.

use gpui::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Axis-aligned bounds in window coordinates (logical pixels).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DebugBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl DebugBounds {
    pub fn from_gpui(bounds: Bounds<Pixels>) -> Self {
        Self {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px <= self.x + self.width && py <= self.y + self.height
    }

    pub fn area(&self) -> f32 {
        self.width.max(0.) * self.height.max(0.)
    }
}

/// Optional layout / style metadata attached to a region.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DebugRegionMeta {
    pub padding: Option<String>,
    pub gap: Option<String>,
    pub notes: Option<String>,
}

/// One inspectable UI region.
#[derive(Clone, Debug, PartialEq)]
pub struct DebugRegion {
    pub id: String,
    pub label: String,
    pub bounds: Option<DebugBounds>,
    pub meta: DebugRegionMeta,
}

struct Registry {
    regions: Vec<DebugRegion>,
    selected: Option<String>,
    /// Bounds from the previous frame, reapplied when a region re-registers
    /// before its measuring canvas has run this frame.
    stale_bounds: HashMap<String, DebugBounds>,
    /// Generation bumped each clear so callers can detect a new frame.
    generation: u64,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            regions: Vec::new(),
            selected: None,
            stale_bounds: HashMap::new(),
            generation: 0,
        }
    }
}

fn registry() -> &'static Mutex<Registry> {
    static REG: OnceLock<Mutex<Registry>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(Registry::default()))
}

/// Drop all regions (call once per frame before re-registering).
///
/// Measured bounds are kept in [`Registry::stale_bounds`] so the Elements list
/// still shows sizes until canvases remeasure this frame.
pub fn clear_regions() {
    let mut reg = registry().lock();
    let mut stale = HashMap::new();
    for region in reg.regions.drain(..) {
        if let Some(b) = region.bounds {
            stale.insert(region.id, b);
        }
    }
    reg.stale_bounds = stale;
    reg.generation = reg.generation.wrapping_add(1);
}

/// Register or replace a region by id.
///
/// When `bounds` is `None`, reuses last-frame bounds for the same id if any.
pub fn register_region(
    id: impl Into<String>,
    label: impl Into<String>,
    bounds: Option<DebugBounds>,
    meta: DebugRegionMeta,
) {
    let id = id.into();
    let label = label.into();
    let mut reg = registry().lock();
    let bounds = bounds.or_else(|| reg.stale_bounds.get(&id).copied());
    if let Some(b) = bounds {
        reg.stale_bounds.insert(id.clone(), b);
    }
    if let Some(existing) = reg.regions.iter_mut().find(|r| r.id == id) {
        existing.label = label;
        if bounds.is_some() {
            existing.bounds = bounds;
        }
        existing.meta = meta;
    } else {
        reg.regions.push(DebugRegion {
            id,
            label,
            bounds,
            meta,
        });
    }
}

/// Convenience: register with empty meta / no bounds yet.
pub fn debug_region(id: impl Into<String>, label: impl Into<String>) {
    register_region(id, label, None, DebugRegionMeta::default());
}

/// Invisible full-size canvas that registers `id` and refreshes window-relative bounds.
///
/// Place as a child of a `.relative()` container that defines the region.
pub fn region_bounds_sensor(
    id: impl Into<String>,
    label: impl Into<String>,
    meta: DebugRegionMeta,
) -> impl IntoElement {
    let id = id.into();
    let label = label.into();
    register_region(id.clone(), label.clone(), None, meta.clone());
    canvas(
        {
            let id = id.clone();
            let label = label.clone();
            let meta = meta.clone();
            move |bounds, _window, _cx| {
                register_region(id, label, Some(DebugBounds::from_gpui(bounds)), meta);
            }
        },
        |_bounds, _meta, _window, _cx| {},
    )
    .absolute()
    .size_full()
    .opacity(0.)
}

/// Snapshot of all regions (newest registration order).
pub fn region_list() -> Vec<DebugRegion> {
    registry().lock().regions.clone()
}

pub fn selected_region() -> Option<DebugRegion> {
    let reg = registry().lock();
    let id = reg.selected.as_ref()?;
    reg.regions.iter().find(|r| &r.id == id).cloned()
}

pub fn set_selected(id: Option<String>) {
    registry().lock().selected = id;
}

/// Pick the smallest-area region containing `(x, y)`, if any.
pub fn select_region_at(x: f32, y: f32) -> Option<DebugRegion> {
    let mut reg = registry().lock();
    let mut best: Option<(usize, f32)> = None;
    for (i, region) in reg.regions.iter().enumerate() {
        let Some(b) = region.bounds else { continue };
        if !b.contains(x, y) {
            continue;
        }
        let area = b.area();
        match best {
            None => best = Some((i, area)),
            Some((_, a)) if area < a => best = Some((i, area)),
            _ => {}
        }
    }
    let Some((i, _)) = best else {
        return None;
    };
    let region = reg.regions[i].clone();
    reg.selected = Some(region.id.clone());
    Some(region)
}
