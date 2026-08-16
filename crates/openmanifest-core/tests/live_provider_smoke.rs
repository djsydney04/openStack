use openmanifest_core::{
    build_provider_probe_plan, execute_provider_probe, HttpProviderTransport, ProcessEnvironment,
    ProviderRequestStatus,
};
use std::collections::HashMap;

#[test]
#[ignore = "requires VERCEL_TOKEN and live network access"]
fn vercel_live_access_probe() {
    assert_live_provider("vercel");
}

#[test]
#[ignore = "requires SUPABASE_ACCESS_TOKEN and live network access"]
fn supabase_live_access_probe() {
    assert_live_provider("supabase");
}

#[test]
#[ignore = "requires NEON_API_KEY and live network access"]
fn neon_live_access_probe() {
    assert_live_provider("neon");
}

#[test]
#[ignore = "requires RAILWAY_TOKEN and live network access"]
fn railway_live_access_probe() {
    assert_live_provider("railway");
}

fn assert_live_provider(provider: &str) {
    let plan = build_provider_probe_plan(Some(provider), &HashMap::new()).expect("probe plan");
    let report = execute_provider_probe(
        &plan,
        &HttpProviderTransport::default(),
        &ProcessEnvironment,
    );
    assert_eq!(report.results.len(), 1);
    assert_eq!(
        report.results[0].status,
        ProviderRequestStatus::Applied,
        "{}",
        report.results[0].message
    );
}
