//! Button choice for one harness row on the provider sheet.

use halo::{RowAction, RowActionInput, row_action};

fn input(
    id: &'static str,
    installed: bool,
    needs_upgrade: bool,
    state_blocked: bool,
    auth_ready: bool,
    login_waiting: bool,
) -> RowActionInput<'static> {
    RowActionInput {
        id,
        installed,
        needs_upgrade,
        state_blocked,
        auth_ready,
        login_waiting,
    }
}

#[test]
fn missing_binary_installs() {
    assert_eq!(
        row_action(&input("pi", false, false, false, true, false)),
        RowAction::Install
    );
}

#[test]
fn wrong_version_upgrades() {
    assert_eq!(
        row_action(&input("opencode", true, true, false, true, false)),
        RowAction::Upgrade
    );
}

#[test]
fn state_blocked_repairs_ownership_instead_of_reinstalling() {
    assert_eq!(
        row_action(&input("opencode", true, true, true, true, false)),
        RowAction::FixOwnership
    );
}

#[test]
fn antigravity_waiting_cancels() {
    assert_eq!(
        row_action(&input("antigravity", true, false, false, false, true)),
        RowAction::Cancel
    );
}

#[test]
fn cursor_key_missing_connects() {
    assert_eq!(
        row_action(&input("cursor", true, false, false, false, false)),
        RowAction::Connect
    );
}

#[test]
fn ready_harness_has_no_button() {
    assert_eq!(
        row_action(&input("pi", true, false, false, true, false)),
        RowAction::None
    );
    assert_eq!(
        row_action(&input("antigravity", true, false, false, true, false)),
        RowAction::None
    );
}

#[test]
fn pi_does_not_treat_a_missing_model_key_as_connect() {
    assert_eq!(
        row_action(&input("pi", true, false, false, false, false)),
        RowAction::None
    );
}
