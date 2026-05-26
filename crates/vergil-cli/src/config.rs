//! `properties.yaml` schema v1.

use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PropertiesFile {
    /// Schema version. v1 is the only supported value in Phase 1.
    pub version: u32,
    /// Optional path to the property contract (relative to the project dir).
    /// Defaults to `test/Properties.t.sol`.
    #[serde(default)]
    pub property_contract: Option<PropertyContractRef>,
    /// List of properties to verify.
    pub properties: Vec<PropertyEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PropertyContractRef {
    pub name: String,
    pub path: String,
    /// Constructor literals, e.g. `["1000000 ether"]`. Defaults to empty.
    #[serde(default)]
    pub constructor_args: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PropertyEntry {
    /// Halmos `check_*` function name.
    pub name: String,
    /// Parameters in source order (used by the counterexample emitter).
    pub params: Vec<PropertyParam>,
    /// Source file used by SMTChecker (relative to project dir).
    /// Defaults to `src/<first .sol file>`.
    #[serde(default)]
    pub smtchecker_source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PropertyParam {
    pub name: String,
    #[serde(rename = "type")]
    pub solidity_type: String,
}

pub fn load(path: &Path) -> Result<PropertiesFile, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let file: PropertiesFile =
        serde_yaml::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    if file.version != 1 {
        return Err(format!(
            "unsupported properties.yaml version {}: Phase 1 expects version 1",
            file.version
        ));
    }
    Ok(file)
}
