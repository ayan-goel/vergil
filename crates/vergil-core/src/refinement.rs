//! Refinement drivers — repair_code, refine_spec, decompose. Each takes
//! the relevant context (intent, current spec, contract source, counter-
//! example trace) and returns a typed structured response the CEGIS loop
//! can apply.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use vergil_llm::prompts::{DECOMPOSE, REFINE_SPEC, REPAIR_CODE};
use vergil_llm::{LlmError, LlmProvider, Message, Role, StructuredRequest};

use crate::synthesis::SpecCandidate;

#[derive(Debug, Clone)]
pub struct RefinementConfig {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl RefinementConfig {
    pub fn default_for_anthropic() -> Self {
        Self {
            model: "claude-opus-4-7".to_string(),
            max_tokens: 4096,
            temperature: 0.0,
        }
    }

    pub fn for_tests() -> Self {
        Self {
            model: "mock".to_string(),
            max_tokens: 1024,
            temperature: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodePatch {
    pub file: String,
    pub before: String,
    pub after: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeRepairPlan {
    #[serde(default)]
    pub patches: Vec<CodePatch>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefinedSpec {
    pub name: String,
    pub halmos: String,
    #[serde(default)]
    pub smtchecker: String,
    pub delta: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecomposeSubProperty {
    pub name: String,
    pub halmos: String,
    #[serde(default)]
    pub smtchecker: String,
    pub implies: String,
    pub cost_class: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecomposeResult {
    pub sub_properties: Vec<DecomposeSubProperty>,
    pub composition_argument: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RefineError {
    #[error("prompt render: {0}")]
    Prompt(vergil_llm::prompts::PromptError),
    #[error("llm: {0}")]
    Llm(#[from] LlmError),
    #[error("schema: {0}")]
    Schema(String),
}

pub struct Refiner {
    provider: Arc<dyn LlmProvider>,
    cfg: RefinementConfig,
}

impl Refiner {
    pub fn new(provider: Arc<dyn LlmProvider>, cfg: RefinementConfig) -> Self {
        Self { provider, cfg }
    }

    pub async fn repair_code(
        &self,
        intent: &str,
        spec: &SpecCandidate,
        contract_source: &str,
        counterexample_trace: &str,
    ) -> Result<CodeRepairPlan, RefineError> {
        let prompt = render_repair_code(intent, spec, contract_source, counterexample_trace)
            .map_err(RefineError::Prompt)?;
        let value = self
            .call_structured(prompt, "CodeRepairPlan", repair_schema())
            .await?;
        serde_json::from_value::<CodeRepairPlan>(value)
            .map_err(|e| RefineError::Schema(format!("{e}")))
    }

    pub async fn refine_spec(
        &self,
        intent: &str,
        spec: &SpecCandidate,
        contract_source: &str,
        counterexample_trace: &str,
    ) -> Result<RefinedSpec, RefineError> {
        let prompt = render_refine_spec(intent, spec, contract_source, counterexample_trace)
            .map_err(RefineError::Prompt)?;
        let value = self
            .call_structured(prompt, "RefinedSpec", refine_schema())
            .await?;
        // The prompt nests under {"refined": {...}}; pull it out.
        let refined = value.get("refined").cloned().unwrap_or(value);
        serde_json::from_value::<RefinedSpec>(refined)
            .map_err(|e| RefineError::Schema(format!("{e}")))
    }

    pub async fn decompose(
        &self,
        intent: &str,
        spec: &SpecCandidate,
        timeout_context: &str,
    ) -> Result<DecomposeResult, RefineError> {
        let prompt =
            render_decompose(intent, spec, timeout_context).map_err(RefineError::Prompt)?;
        let value = self
            .call_structured(prompt, "Decompose", decompose_schema())
            .await?;
        serde_json::from_value::<DecomposeResult>(value)
            .map_err(|e| RefineError::Schema(format!("{e}")))
    }

    async fn call_structured(
        &self,
        prompt: String,
        schema_name: &str,
        schema: serde_json::Value,
    ) -> Result<serde_json::Value, RefineError> {
        let req = StructuredRequest {
            model: self.cfg.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: prompt,
            }],
            system: Some(
                "You refine a Vergil verification artifact. Return ONLY the JSON object the user prompt's schema describes."
                    .to_string(),
            ),
            temperature: self.cfg.temperature,
            max_tokens: self.cfg.max_tokens,
            schema_name: schema_name.to_string(),
            schema,
        };
        let resp = self.provider.complete_structured(req).await?;
        Ok(resp.value)
    }
}

fn render_repair_code(
    intent: &str,
    spec: &SpecCandidate,
    contract_source: &str,
    counterexample_trace: &str,
) -> Result<String, vergil_llm::prompts::PromptError> {
    let spec_source = spec_source_blob(spec);
    let mut vars: BTreeMap<&str, &str> = BTreeMap::new();
    vars.insert("intent", intent);
    vars.insert("spec_source", &spec_source);
    vars.insert("contract_source", contract_source);
    vars.insert("counterexample_trace", counterexample_trace);
    REPAIR_CODE.render(&vars)
}

fn render_refine_spec(
    intent: &str,
    spec: &SpecCandidate,
    contract_source: &str,
    counterexample_trace: &str,
) -> Result<String, vergil_llm::prompts::PromptError> {
    let spec_source = spec_source_blob(spec);
    let mut vars: BTreeMap<&str, &str> = BTreeMap::new();
    vars.insert("intent", intent);
    vars.insert("spec_source", &spec_source);
    vars.insert("contract_source", contract_source);
    vars.insert("counterexample_trace", counterexample_trace);
    REFINE_SPEC.render(&vars)
}

fn render_decompose(
    intent: &str,
    spec: &SpecCandidate,
    timeout_context: &str,
) -> Result<String, vergil_llm::prompts::PromptError> {
    let spec_source = spec_source_blob(spec);
    let mut vars: BTreeMap<&str, &str> = BTreeMap::new();
    vars.insert("intent", intent);
    vars.insert("spec_source", &spec_source);
    vars.insert("timeout_context", timeout_context);
    DECOMPOSE.render(&vars)
}

fn spec_source_blob(spec: &SpecCandidate) -> String {
    let mut out = String::with_capacity(spec.halmos.len() + spec.smtchecker.len() + 64);
    out.push_str("// Halmos check_ function\n");
    out.push_str(&spec.halmos);
    if !spec.smtchecker.is_empty() {
        out.push_str("\n\n// SMTChecker fragment\n");
        out.push_str(&spec.smtchecker);
    }
    out
}

fn repair_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["patches", "summary"],
        "properties": {
            "patches": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["file", "before", "after", "rationale"],
                    "properties": {
                        "file": { "type": "string" },
                        "before": { "type": "string" },
                        "after": { "type": "string" },
                        "rationale": { "type": "string" }
                    }
                }
            },
            "summary": { "type": "string" }
        }
    })
}

fn refine_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["refined"],
        "properties": {
            "refined": {
                "type": "object",
                "required": ["name", "halmos", "delta"],
                "properties": {
                    "name": { "type": "string" },
                    "halmos": { "type": "string" },
                    "smtchecker": { "type": "string" },
                    "delta": { "type": "string" }
                }
            }
        }
    })
}

fn decompose_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["sub_properties", "composition_argument"],
        "properties": {
            "sub_properties": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["name", "halmos", "implies", "cost_class"],
                    "properties": {
                        "name": { "type": "string" },
                        "halmos": { "type": "string" },
                        "smtchecker": { "type": "string" },
                        "implies": { "type": "string" },
                        "cost_class": { "type": "string", "enum": ["trivial", "cheap", "medium"] }
                    }
                }
            },
            "composition_argument": { "type": "string" }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spec() -> SpecCandidate {
        SpecCandidate {
            name: "check_x".into(),
            halmos: "function check_x() public {}".into(),
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: true,
        }
    }

    #[test]
    fn renders_repair_inlines_all_inputs() {
        let s = sample_spec();
        let out = render_repair_code("intent", &s, "contract source", "cex trace").unwrap();
        assert!(out.contains("intent"));
        assert!(out.contains("contract source"));
        assert!(out.contains("cex trace"));
        assert!(out.contains("check_x"));
        assert!(!out.contains("{{"));
    }

    #[test]
    fn renders_refine_inlines_all_inputs() {
        let s = sample_spec();
        let out = render_refine_spec("intent", &s, "contract source", "cex trace").unwrap();
        assert!(out.contains("intent"));
        assert!(out.contains("cex trace"));
        assert!(!out.contains("{{"));
    }

    #[test]
    fn renders_decompose_inlines_all_inputs() {
        let s = sample_spec();
        let out = render_decompose("intent", &s, "halmos timeout 60s").unwrap();
        assert!(out.contains("halmos timeout 60s"));
        assert!(out.contains("check_x"));
        assert!(!out.contains("{{"));
    }

    #[test]
    fn code_repair_plan_round_trips_through_json() {
        let p = CodeRepairPlan {
            patches: vec![CodePatch {
                file: "src/Token.sol".into(),
                before: "missing".into(),
                after: "require(allowance >= amount)".into(),
                rationale: "missing auth".into(),
            }],
            summary: "tighten transferFrom".into(),
        };
        let s = serde_json::to_string(&p).unwrap();
        let back: CodeRepairPlan = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn refined_spec_round_trips_through_json() {
        let r = RefinedSpec {
            name: "check_x".into(),
            halmos: "function check_x() {}".into(),
            smtchecker: String::new(),
            delta: "tightened precondition".into(),
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: RefinedSpec = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }
}
