//! Status indicator bar for Intruder attack state.

use gpui::*;

use crate::pages::chrome::{StatusTone, status_pill};

pub fn render_intruder_status(
    running: bool,
    attack_id: Option<&str>,
    message: Option<&str>,
    cx: &App,
) -> impl IntoElement {
    let status_line: SharedString = match (running, attack_id, message) {
        (true, Some(id), Some(msg)) => format!("Running — {id}: {msg}").into(),
        (true, Some(id), None) => format!("Running — {id}").into(),
        (false, Some(id), Some(msg)) => format!("Idle — last {id}: {msg}").into(),
        (false, _, Some(msg)) => format!("Idle — {msg}").into(),
        _ => "Idle".into(),
    };

    status_pill(
        status_line,
        if running {
            StatusTone::Active
        } else {
            StatusTone::Idle
        },
        cx,
    )
}
