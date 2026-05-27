//! Six compile-time-loaded prompt templates that drive Phase 2's closed loop.
//!
//! Each prompt is a [`Prompt`] carrying:
//!   * `name` — stable identifier recorded in the trace.
//!   * `body` — the rendered template text, loaded via `include_str!`.
//!   * `sha` — SHA-256 of `body`, computed once at first access; recorded
//!     in every trace event so a run can be reconstructed from cache.
//!
//! Substitution is intentionally simple — a literal `{{ var }}` (with
//! flexible whitespace) is replaced with the caller-supplied value.
//! Missing variables produce [`PromptError::MissingVar`] rather than
//! silently leaving placeholders in the rendered text. Unused variables
//! are tolerated (e.g. for slices that omit static-analysis context).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use sha2::Digest;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PromptError {
    #[error("missing variable {0} in prompt {1}")]
    MissingVar(String, &'static str),
    #[error("unrendered placeholders left in {0}: {1}")]
    UnrenderedPlaceholder(&'static str, String),
}

/// A prompt template. `body` is a literal string with `{{ var }}` markers
/// that [`render`](Prompt::render) substitutes with caller-supplied values.
pub struct Prompt {
    pub name: &'static str,
    pub body: &'static str,
    sha_cache: OnceLock<[u8; 32]>,
}

impl Prompt {
    pub const fn new(name: &'static str, body: &'static str) -> Self {
        Self {
            name,
            body,
            sha_cache: OnceLock::new(),
        }
    }

    pub fn sha(&self) -> [u8; 32] {
        *self.sha_cache.get_or_init(|| {
            let mut h = sha2::Sha256::new();
            h.update(self.body.as_bytes());
            h.finalize().into()
        })
    }

    pub fn sha_hex(&self) -> String {
        crate::sha_hex(&self.sha())
    }

    /// Substitute every `{{ key }}` in `body` with `vars[key]`. Returns an
    /// error if any placeholder cannot be resolved or if the rendered text
    /// still contains a `{{` marker (defense against schema drift).
    pub fn render(&self, vars: &BTreeMap<&str, &str>) -> Result<String, PromptError> {
        let mut out = String::with_capacity(self.body.len());
        let mut rest = self.body;
        while let Some(open) = rest.find("{{") {
            out.push_str(&rest[..open]);
            let after_open = &rest[open + 2..];
            let close = after_open
                .find("}}")
                .ok_or_else(|| PromptError::UnrenderedPlaceholder(self.name, rest.into()))?;
            let key = after_open[..close].trim();
            let value = vars
                .get(key)
                .ok_or_else(|| PromptError::MissingVar(key.to_string(), self.name))?;
            out.push_str(value);
            rest = &after_open[close + 2..];
        }
        out.push_str(rest);
        if out.contains("{{") {
            return Err(PromptError::UnrenderedPlaceholder(self.name, out));
        }
        Ok(out)
    }
}

macro_rules! load_prompt {
    ($name:ident, $file:literal) => {
        pub static $name: Prompt = Prompt::new(stringify!($name), include_str!($file));
    };
}

load_prompt!(SYNTHESIZE, "prompts/synthesize.txt");
load_prompt!(CRITIQUE, "prompts/critique.txt");
load_prompt!(DIAGNOSE, "prompts/diagnose.txt");
load_prompt!(REPAIR_CODE, "prompts/repair_code.txt");
load_prompt!(REFINE_SPEC, "prompts/refine_spec.txt");
load_prompt!(DECOMPOSE, "prompts/decompose.txt");

pub static ALL: &[&Prompt] = &[
    &SYNTHESIZE,
    &CRITIQUE,
    &DIAGNOSE,
    &REPAIR_CODE,
    &REFINE_SPEC,
    &DECOMPOSE,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&'static str, &'static str)]) -> BTreeMap<&'static str, &'static str> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn all_six_prompts_load() {
        assert_eq!(ALL.len(), 6);
        for p in ALL {
            assert!(!p.body.is_empty(), "prompt {} has empty body", p.name);
        }
    }

    #[test]
    fn sha_is_stable_across_calls() {
        for p in ALL {
            let a = p.sha();
            let b = p.sha();
            assert_eq!(a, b, "{} sha drifted", p.name);
        }
    }

    #[test]
    fn render_substitutes_simple_placeholder() {
        let p = Prompt::new("test", "hello {{ name }}, model {{model}}.");
        let out = p
            .render(&vars(&[("name", "world"), ("model", "claude")]))
            .unwrap();
        assert_eq!(out, "hello world, model claude.");
    }

    #[test]
    fn render_tolerates_unused_vars() {
        let p = Prompt::new("test", "hi {{ a }}");
        let out = p.render(&vars(&[("a", "x"), ("unused", "y")])).unwrap();
        assert_eq!(out, "hi x");
    }

    #[test]
    fn missing_var_is_typed_error() {
        let p = Prompt::new("test", "hi {{ who }}");
        let err = p.render(&vars(&[])).unwrap_err();
        match err {
            PromptError::MissingVar(name, prompt) => {
                assert_eq!(name, "who");
                assert_eq!(prompt, "test");
            }
            other => panic!("expected MissingVar, got {other:?}"),
        }
    }

    #[test]
    fn output_never_contains_unrendered_placeholder() {
        // Build a vars map populated with placeholder fillers for every
        // unique {{ key }} found in each prompt body, then ensure render()
        // produces text with no leftover {{ markers.
        for p in ALL {
            let keys = extract_keys(p.body);
            let placeholders: Vec<(String, String)> = keys
                .iter()
                .map(|k| (k.clone(), format!("<FILLED:{k}>")))
                .collect();
            let mut map: BTreeMap<&str, &str> = BTreeMap::new();
            for (k, v) in &placeholders {
                map.insert(k.as_str(), v.as_str());
            }
            let out = p
                .render(&map)
                .unwrap_or_else(|e| panic!("{} render failed: {e}", p.name));
            assert!(
                !out.contains("{{") && !out.contains("}}"),
                "{} produced leftover braces:\n{out}",
                p.name
            );
        }
    }

    fn extract_keys(body: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = body;
        while let Some(open) = rest.find("{{") {
            let after = &rest[open + 2..];
            if let Some(close) = after.find("}}") {
                let key = after[..close].trim().to_string();
                if !out.contains(&key) {
                    out.push(key);
                }
                rest = &after[close + 2..];
            } else {
                break;
            }
        }
        out
    }
}
