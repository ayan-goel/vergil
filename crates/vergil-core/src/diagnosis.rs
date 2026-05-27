//! Diagnosis classifier: given a counterexample, the failing spec, and the
//! user's intent, classify the failure as CodeBug | SpecBug | Ambiguous.
//! The verdict routes Slice 13's CEGIS loop: CodeBug → repair_code prompt,
//! SpecBug → refine_spec prompt, Ambiguous → exit + surface to user.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use vergil_llm::prompts::DIAGNOSE;
use vergil_llm::{LlmProvider, Message, Role, StructuredRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosisClass {
    CodeBug,
    SpecBug,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnosis {
    pub class: DiagnosisClass,
    pub rationale: String,
    #[serde(default)]
    pub fix_hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DiagnosisConfig {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl DiagnosisConfig {
    pub fn default_for_anthropic() -> Self {
        Self {
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 1024,
            temperature: 0.0,
        }
    }

    pub fn for_tests() -> Self {
        Self {
            model: "mock".to_string(),
            max_tokens: 512,
            temperature: 0.0,
        }
    }
}

pub struct Diagnostician {
    provider: Arc<dyn LlmProvider>,
    cfg: DiagnosisConfig,
}

impl Diagnostician {
    pub fn new(provider: Arc<dyn LlmProvider>, cfg: DiagnosisConfig) -> Self {
        Self { provider, cfg }
    }

    pub async fn diagnose(
        &self,
        intent: &str,
        spec_source: &str,
        counterexample_trace: &str,
    ) -> Diagnosis {
        let prompt = match render(intent, spec_source, counterexample_trace) {
            Ok(p) => p,
            Err(e) => return ambiguous_with(format!("prompt render failed: {e}")),
        };
        let req = StructuredRequest {
            model: self.cfg.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: prompt,
            }],
            system: Some(
                "You are a triage engineer for verification failures. Return ONLY the JSON object the user prompt's schema describes."
                    .to_string(),
            ),
            temperature: self.cfg.temperature,
            max_tokens: self.cfg.max_tokens,
            schema_name: "Diagnosis".to_string(),
            schema: diagnosis_schema(),
        };
        match self.provider.complete_structured(req).await {
            Ok(resp) => match serde_json::from_value::<Diagnosis>(resp.value) {
                Ok(d) => d,
                Err(e) => ambiguous_with(format!("diagnosis JSON shape: {e}")),
            },
            Err(e) => ambiguous_with(format!("llm: {e}")),
        }
    }
}

fn render(
    intent: &str,
    spec_source: &str,
    counterexample_trace: &str,
) -> Result<String, vergil_llm::prompts::PromptError> {
    let mut vars: BTreeMap<&str, &str> = BTreeMap::new();
    vars.insert("intent", intent);
    vars.insert("spec_source", spec_source);
    vars.insert("counterexample_trace", counterexample_trace);
    DIAGNOSE.render(&vars)
}

fn diagnosis_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["class", "rationale"],
        "properties": {
            "class": { "type": "string", "enum": ["CodeBug", "SpecBug", "Ambiguous"] },
            "rationale": { "type": "string" },
            "fix_hint": { "type": "string" }
        }
    })
}

fn ambiguous_with(reason: String) -> Diagnosis {
    Diagnosis {
        class: DiagnosisClass::Ambiguous,
        rationale: format!("diagnose failed, defaulting Ambiguous: {reason}"),
        fix_hint: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use vergil_llm::mock::MockProvider;

    #[test]
    fn render_inlines_all_inputs() {
        let out = render(
            "preserve totalSupply",
            "function check_x() { assert(false); }",
            "from=0xAAA to=0xBBB amount=0",
        )
        .unwrap();
        assert!(out.contains("preserve totalSupply"));
        assert!(out.contains("check_x"));
        assert!(out.contains("0xAAA"));
        assert!(!out.contains("{{"));
    }

    #[tokio::test]
    async fn missing_fixture_defaults_to_ambiguous_with_clear_reason() {
        let provider = Arc::new(MockProvider::new(PathBuf::from("/nonexistent-vergil-dir")));
        let d = Diagnostician::new(provider, DiagnosisConfig::for_tests());
        let diag = d
            .diagnose("intent", "function check_x() {}", "concrete inputs")
            .await;
        assert_eq!(diag.class, DiagnosisClass::Ambiguous);
        assert!(diag.rationale.contains("diagnose failed"));
    }

    #[test]
    fn diagnosis_round_trips_through_json() {
        let d = Diagnosis {
            class: DiagnosisClass::CodeBug,
            rationale: "missing allowance check".into(),
            fix_hint: Some("add require".into()),
        };
        let s = serde_json::to_string(&d).unwrap();
        let back: Diagnosis = serde_json::from_str(&s).unwrap();
        assert_eq!(back, d);
    }
}
