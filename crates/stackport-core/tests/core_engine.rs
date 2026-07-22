use stackport_core::analysis::analyze_portability;
use stackport_core::apply::{apply_plan, DryRunAdapter};
use stackport_core::diff::{diff_state, DiffAction};
use stackport_core::importers::{
    import_supabase, import_vercel, SupabaseProject, VercelEnvironmentVariable, VercelProject,
};
use stackport_core::manifest::{
    validate_manifest, Application, Capability, Manifest, MigrationScope, Resource, ResourceKind,
    MANIFEST_SCHEMA_VERSION,
};
use stackport_core::planner::{create_plan, PlanAction};
use stackport_core::rpc::{handle_rpc_request, JsonRpcRequest};
use stackport_core::state::{StackState, StateResource};

#[test]
fn validates_manifest_graph_and_secret_references() {
    let manifest = sample_manifest();
    let report = validate_manifest(&manifest).expect("manifest should validate");
    assert!(report.valid);
    assert!(report.warnings.is_empty());
}

#[test]
fn rejects_inline_secret_values() {
    let mut manifest = sample_manifest();
    manifest.resources[0].properties = serde_json::json!({
        "api_token": "plaintext-token-value"
    });
    let error = validate_manifest(&manifest).expect_err("inline secret should fail");
    assert!(error.to_string().contains("likely secret value"));
}

#[test]
fn imports_vercel_without_secret_values() {
    let manifest = import_vercel(VercelProject {
        name: "next-app".to_string(),
        framework: Some("nextjs".to_string()),
        domains: vec!["example.com".to_string()],
        environment: vec![VercelEnvironmentVariable {
            key: "DATABASE_URL".to_string(),
            target: Some("production".to_string()),
        }],
    });

    validate_manifest(&manifest).expect("imported manifest should validate");
    assert_eq!(manifest.application.name, "next-app");
    assert_eq!(manifest.resources.len(), 2);
    assert_eq!(
        manifest.variables["DATABASE_URL"].secret_ref.as_deref(),
        Some("vercel:DATABASE_URL")
    );
}

#[test]
fn imports_supabase_resources() {
    let manifest = import_supabase(SupabaseProject {
        project_ref: "abc123".to_string(),
        name: Some("platform".to_string()),
        database: true,
        auth: true,
        storage_buckets: vec!["avatars".to_string()],
        edge_functions: vec!["webhook".to_string()],
    });

    validate_manifest(&manifest).expect("imported manifest should validate");
    assert_eq!(manifest.resources.len(), 4);
    assert!(manifest
        .resources
        .iter()
        .any(|resource| resource.id == "auth"));
}

#[test]
fn analyzes_missing_target_capabilities() {
    let report = analyze_portability(&sample_manifest(), "vercel", None).expect("analysis");
    assert!(!report.portable);
    assert!(report
        .resources
        .iter()
        .any(|resource| resource.resource_id == "database" && !resource.portable));
}

#[test]
fn creates_partial_plan_with_scope_warnings() {
    let plan = create_plan(
        &sample_manifest(),
        "render",
        Some(MigrationScope {
            include_resources: vec!["web".to_string()],
            exclude_resources: vec![],
        }),
    )
    .expect("plan");

    assert!(plan.partial);
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].action, PlanAction::Create);
    assert_eq!(plan.warnings.len(), 1);
}

#[test]
fn diffs_manifest_against_state() {
    let manifest = sample_manifest();
    let state = StackState {
        schema_version: "stackport-state/v1".to_string(),
        provider: "render".to_string(),
        resources: vec![StateResource {
            id: "old".to_string(),
            provider_id: "old-1".to_string(),
            fingerprint: "stale".to_string(),
        }],
    };

    let diff = diff_state(&manifest, &state).expect("diff");
    assert!(diff
        .changes
        .iter()
        .any(|change| change.action == DiffAction::Create));
    assert!(diff
        .changes
        .iter()
        .any(|change| change.action == DiffAction::Delete));
}

#[test]
fn dry_run_apply_plans_creates_and_skips_unsupported() {
    let plan = create_plan(&sample_manifest(), "vercel", None).expect("plan");
    let report = apply_plan(&plan, &DryRunAdapter, true);
    assert!(report.dry_run);
    assert_eq!(report.results.len(), plan.steps.len());
}

#[test]
fn handles_json_rpc_version_and_plan() {
    let version_response = handle_rpc_request(JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: serde_json::json!("1"),
        method: "stackport.version".to_string(),
        params: serde_json::json!({}),
    });
    assert!(version_response.error.is_none());

    let plan_response = handle_rpc_request(JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: serde_json::json!("2"),
        method: "plan.create".to_string(),
        params: serde_json::json!({
            "manifest": sample_manifest(),
            "target_provider": "render"
        }),
    });
    assert!(plan_response.error.is_none());
    assert_eq!(
        plan_response.result.unwrap()["target_provider"],
        serde_json::json!("render")
    );
}

fn sample_manifest() -> Manifest {
    Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        application: Application {
            name: "demo".to_string(),
            description: None,
            tags: vec![],
        },
        resources: vec![
            Resource {
                id: "database".to_string(),
                kind: ResourceKind::Database,
                provider: Some("supabase".to_string()),
                capabilities: vec![Capability::Postgres],
                depends_on: vec![],
                properties: serde_json::json!({ "engine": "postgres" }),
            },
            Resource {
                id: "web".to_string(),
                kind: ResourceKind::WebService,
                provider: Some("vercel".to_string()),
                capabilities: vec![Capability::Build],
                depends_on: vec!["database".to_string()],
                properties: serde_json::json!({ "framework": "nextjs" }),
            },
        ],
        variables: Default::default(),
    }
}
