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
    let initial_count = stub.get_contexts().unwrap().len();

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
    assert_eq!(contexts.len(), initial_count + 1);
    let found = contexts.iter().find(|c| c.id == "env-1").unwrap();
    assert_eq!(found.name, "Initial Name");

    // Rename environment
    let mut renamed = ctx;
    renamed.name = "Production".into();
    stub.save_context(renamed).unwrap();
    let contexts = stub.get_contexts().unwrap();
    assert_eq!(contexts.len(), initial_count + 1);
    let found = contexts.iter().find(|c| c.id == "env-1").unwrap();
    assert_eq!(found.name, "Production");
}
