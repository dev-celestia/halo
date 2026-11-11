//! Stub-backend tests.

use super::*;

#[test]
fn degraded_stub_names_the_boot_error() {
    let err =
        StubBackend::degraded("open database /tmp/halo.db: unable to open").agent_unavailable();
    assert!(
        err.to_string().contains("unable to open"),
        "boot error must survive into the command error: {err}"
    );
}

#[test]
fn plain_stub_keeps_the_generic_message() {
    let err = StubBackend::new().agent_unavailable();
    assert_eq!(err.to_string(), "Halo Agent is unavailable in stub mode");
}
