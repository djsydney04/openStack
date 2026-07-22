use stackport_sdk::{Manifest, StackportEngine};

#[test]
fn rust_sdk_uses_core_engine() {
    let manifest: Manifest =
        serde_json::from_str(include_str!("../../../fixtures/manifest.basic.json")).unwrap();
    let engine = StackportEngine;

    assert!(engine.validate(&manifest).unwrap().valid);
    let plan = engine.plan(&manifest, "render", None).unwrap();
    assert_eq!(plan.target_provider, "render");
}
