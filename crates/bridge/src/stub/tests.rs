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

#[test]
fn stub_backend_context_rename_and_lifecycle() {
    let stub = StubBackend::new();
    let now = "2026-01-01T00:00:00Z".to_string();
    let ctx = ContextDto {
        id: "env-1".into(),
        name: "Initial Name".into(),
        variables: "{}".into(),
        created_at: now.clone(),
        updated_at: now,
    };
    stub.save_context(ctx.clone()).unwrap();
    let contexts = stub.get_contexts().unwrap();
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0].name, "Initial Name");

    // Rename environment
    let mut renamed = ctx;
    renamed.name = "Production".into();
    stub.save_context(renamed).unwrap();
    let contexts = stub.get_contexts().unwrap();
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0].name, "Production");
}
