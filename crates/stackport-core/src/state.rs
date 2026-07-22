use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StackState {
    pub schema_version: String,
    pub provider: String,
    #[serde(default)]
    pub resources: Vec<StateResource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateResource {
    pub id: String,
    pub provider_id: String,
    pub fingerprint: String,
}
