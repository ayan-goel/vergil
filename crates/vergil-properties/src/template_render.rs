//! Template rendering: substitute `{{key}}` placeholders into encoding
//! templates and fixtures.
//!
//! Phase-1 keeps this deliberately simple: a literal `{{key}}` is replaced
//! by `ctx.get("key")`. No conditionals, no loops, no escaping. The bigger
//! reason not to pull in Handlebars/Tera is the rendered output is
//! immediately compiled by `solc` — a templating bug surfaces as a build
//! error, not a silent miscompile, so the rendering layer can stay dumb.
//!
//! When future templates need conditionals (e.g. emit a `nonReentrant`
//! modifier only when the target contract has one), the right move is to
//! resolve the condition in Rust before rendering and pass the resolved
//! substring through `ctx` — not to grow the renderer.

use std::collections::BTreeMap;

use thiserror::Error;

#[derive(Debug, Clone, Default)]
pub struct RenderContext {
    vars: BTreeMap<String, String>,
}

impl RenderContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.vars.insert(k.into(), v.into());
        self
    }

    /// Mutating variant of [`set`] — useful when bindings come from a
    /// dynamic source (e.g. per-PoC YAML) and a consuming builder
    /// wouldn't compose naturally.
    pub fn insert(&mut self, k: impl Into<String>, v: impl Into<String>) {
        self.vars.insert(k.into(), v.into());
    }

    pub fn from_pairs<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let mut ctx = Self::default();
        for (k, v) in pairs {
            ctx.vars.insert(k.into(), v.into());
        }
        ctx
    }

    pub fn get(&self, k: &str) -> Option<&str> {
        self.vars.get(k).map(String::as_str)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RenderError {
    #[error("template references undefined variable `{0}`")]
    UndefinedVariable(String),
    #[error("template has an unterminated `{{` (no matching `}}`)")]
    Unterminated,
}

/// Substitute `{{ key }}` (whitespace tolerated) with `ctx.get("key")`.
/// Returns [`RenderError::UndefinedVariable`] if any key is missing —
/// silently rendering an empty string would produce subtly broken Solidity.
pub fn render(template: &str, ctx: &RenderContext) -> Result<String, RenderError> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let close = after.find("}}").ok_or(RenderError::Unterminated)?;
        let key = after[..close].trim();
        let value = ctx
            .get(key)
            .ok_or_else(|| RenderError::UndefinedVariable(key.to_string()))?;
        out.push_str(value);
        rest = &after[close + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_single_var() {
        let ctx = RenderContext::new().set("name", "MyToken");
        let out = render("contract {{name}} {}", &ctx).unwrap();
        assert_eq!(out, "contract MyToken {}");
    }

    #[test]
    fn tolerates_whitespace_inside_braces() {
        let ctx = RenderContext::new().set("name", "X");
        assert_eq!(render("{{ name }}", &ctx).unwrap(), "X");
        assert_eq!(render("{{name}}", &ctx).unwrap(), "X");
        assert_eq!(render("{{  name  }}", &ctx).unwrap(), "X");
    }

    #[test]
    fn multiple_vars_in_order() {
        let ctx = RenderContext::from_pairs([("a", "A"), ("b", "B")]);
        assert_eq!(render("{{a}}-{{b}}-{{a}}", &ctx).unwrap(), "A-B-A");
    }

    #[test]
    fn no_vars_passes_through() {
        let ctx = RenderContext::new();
        assert_eq!(render("plain text", &ctx).unwrap(), "plain text");
    }

    #[test]
    fn undefined_var_is_error() {
        let ctx = RenderContext::new();
        let err = render("{{missing}}", &ctx).unwrap_err();
        assert_eq!(err, RenderError::UndefinedVariable("missing".to_string()));
    }

    #[test]
    fn unterminated_brace_is_error() {
        let ctx = RenderContext::new().set("x", "y");
        let err = render("{{x", &ctx).unwrap_err();
        assert_eq!(err, RenderError::Unterminated);
    }

    #[test]
    fn from_pairs_accepts_string_types() {
        let ctx = RenderContext::from_pairs([("k", "v".to_string())]);
        assert_eq!(ctx.get("k"), Some("v"));
    }

    #[test]
    fn insert_mutates_in_place() {
        let mut ctx = RenderContext::new();
        ctx.insert("k", "v");
        ctx.insert("k2", "v2");
        // Overwrite an existing key.
        ctx.insert("k", "overwritten");
        assert_eq!(ctx.get("k"), Some("overwritten"));
        assert_eq!(ctx.get("k2"), Some("v2"));
    }
}
