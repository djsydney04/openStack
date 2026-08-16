use openmanifest_sdk::{AppManifest, Manifest, OpenManifestEngine};

#[test]
fn rust_sdk_uses_core_engine() {
    let manifest: Manifest =
        serde_json::from_str(include_str!("../../../fixtures/manifest.basic.json")).unwrap();
    let engine = OpenManifestEngine;

    assert!(engine.validate(&manifest).unwrap().valid);
    let plan = engine.plan(&manifest, "render", None).unwrap();
    assert_eq!(plan.target_provider, "render");

    let app_manifest: AppManifest =
        serde_yaml::from_str(include_str!("../../../fixtures/app.openmanifest.yaml")).unwrap();
    assert!(
        engine
            .validate_app_manifest(&app_manifest, Some("production"))
            .unwrap()
            .valid
    );
}
