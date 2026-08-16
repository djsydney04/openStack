use crate::manifest::{resource_fingerprint, Resource};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const STATE_SCHEMA_VERSION: &str = "openmanifest-state/v1";
pub const LEGACY_STATE_SCHEMA_VERSION: &str = "stackport-state/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StackState {
    pub schema_version: String,
    pub provider: String,
    #[serde(default)]
    pub providers: Vec<StateProvider>,
    #[serde(default)]
    pub resources: Vec<StateResource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateResource {
    pub id: String,
    #[serde(default)]
    pub provider: Option<String>,
    pub provider_id: String,
    #[serde(default)]
    pub provider_resource: Option<String>,
    pub fingerprint: String,
    #[serde(default)]
    pub identifiers: HashMap<String, String>,
    #[serde(default)]
    pub last_applied: Option<serde_json::Value>,
    #[serde(default)]
    pub secrets: Vec<StateSecretRef>,
}

impl StateResource {
    pub fn from_applied(
        resource: &Resource,
        provider_id: String,
        provider_resource: Option<String>,
        identifiers: HashMap<String, String>,
        secrets: Vec<StateSecretRef>,
    ) -> Result<Self, String> {
        Ok(Self {
            id: resource.id.clone(),
            provider: resource.provider.clone(),
            provider_id,
            provider_resource,
            fingerprint: resource_fingerprint(resource),
            identifiers,
            last_applied: Some(
                serde_json::to_value(resource)
                    .map_err(|err| format!("failed to serialize applied resource: {err}"))?,
            ),
            secrets,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateProvider {
    pub name: String,
    pub account_id: Option<String>,
    pub environment_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateSecretRef {
    pub name: String,
    pub provider_ref: String,
}
