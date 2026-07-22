use serde::{Deserialize, Serialize};

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
    pub secrets: Vec<StateSecretRef>,
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
