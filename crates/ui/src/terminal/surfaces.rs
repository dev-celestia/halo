//! One VT surface entity per live PTY session.
//!
//! The map is shared (`Rc<RefCell<…>>`) so page code can hand a clone to click
//! handlers without borrowing the whole [`ToolEditors`] bag, and so lazily
//! created surfaces outlive the frame that made them.
//!
//! [`ToolEditors`]: crate::ToolEditors

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, AppContext as _, Entity};
use halo_terminal_ui::{TerminalHost, TerminalSurface};

use super::BridgeTerminalHost;

/// Session-id → surface map, cheap to clone and safe to capture.
#[derive(Clone)]
pub struct TerminalSurfaces {
    host: Arc<dyn TerminalHost>,
    map: Rc<RefCell<HashMap<String, Entity<TerminalSurface>>>>,
}

impl Default for TerminalSurfaces {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalSurfaces {
    pub fn new() -> Self {
        Self {
            host: Arc::new(BridgeTerminalHost),
            map: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// The surface already bound to `session`, if any.
    pub fn get(&self, session: &str) -> Option<Entity<TerminalSurface>> {
        self.map.borrow().get(session).cloned()
    }

    /// The surface for `session`, creating and binding it on first use.
    ///
    /// Creation runs outside the map borrow: `cx.new` can re-enter rendering
    /// and the closure must not hold the `RefCell` across it.
    pub fn get_or_create(&self, session: &str, cx: &mut App) -> Entity<TerminalSurface> {
        if let Some(entity) = self.get(session) {
            return entity;
        }
        let host = self.host.clone();
        let entity = cx.new(|cx| TerminalSurface::new(host, cx));
        let id = session.to_string();
        entity.update(cx, |surface, _| surface.bind(Some(id.clone())));
        self.map.borrow_mut().insert(id, entity.clone());
        entity
    }

    /// Drop surfaces whose session no longer exists in any work tab.
    pub fn retain(&self, live: &[String]) {
        self.map
            .borrow_mut()
            .retain(|session, _| live.iter().any(|id| id == session));
    }

    /// Number of live surfaces (diagnostics / tests).
    pub fn len(&self) -> usize {
        self.map.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.borrow().is_empty()
    }
}
