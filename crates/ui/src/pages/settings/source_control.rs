//! Source-control section — commit style, sign-off, and background sync.

use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::settings::widgets::{chip, row, rows, section, switch};

const FETCH_CHOICES: [u16; 5] = [0, 5, 10, 15, 30];

pub(in crate::pages::settings) fn render(state: &BridgeState, cx: &App) -> impl IntoElement {
    let settings = state.coder_settings.clone();

    let mut fetch_chips = div().flex().flex_wrap().gap_2();
    for minutes in FETCH_CHOICES {
        let label = if minutes == 0 {
            "Off".to_string()
        } else {
            format!("{minutes} min")
        };
        fetch_chips = fetch_chips.child(chip(
            SharedString::from(format!("coder-auto-fetch-{minutes}")),
            label,
            settings.auto_fetch_minutes == minutes,
            move |cx| {
                Bridge::update_coder_settings(cx, move |s| s.auto_fetch_minutes = minutes);
            },
        ));
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(section(
            "Commit style",
            "How the Source Control panel writes messages for you.",
            rows().child(row(
                "Sign-off",
                "Append a Signed-off-by trailer to every commit.",
                switch("coder-sign-off", settings.sign_off, |next, cx| {
                    Bridge::update_coder_settings(cx, move |s| s.sign_off = next);
                }),
                cx,
            )),
            cx,
        ))
        .child(section(
            "Sync",
            "Background fetching and the guard on history-rewriting pushes.",
            rows()
                .child(row(
                    "Auto-fetch",
                    "Refresh ahead / behind counts while a project is open.",
                    fetch_chips,
                    cx,
                ))
                .child(row(
                    "Confirm force push",
                    "Require an explicit confirmation before --force.",
                    switch(
                        "coder-confirm-force-push",
                        settings.confirm_force_push,
                        |next, cx| {
                            Bridge::update_coder_settings(cx, move |s| {
                                s.confirm_force_push = next;
                            });
                        },
                    ),
                    cx,
                )),
            cx,
        ))
}
