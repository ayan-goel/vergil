//! Stratified verdict formatter — SPEC §3.6 / V1.5 Phase 6 Slice 5.
//!
//! The product's honest core. V1.5 ships **no single green/red bit
//! and no security score**. Instead the verdict is a three-state
//! headline plus a four-section artifact that ALWAYS names what was
//! not checked.
//!
//! Three-state headline:
//! - **Refuted (red)** — a counterexample exists. Highest-confidence,
//!   highest-value output. Surfaced the instant it's found (Slice 6's
//!   streaming).
//! - **Verified-in-scope (the honest green)** — every checked property
//!   returned UNSAT from the solver AND the scope statement +
//!   "Not checked" section name precisely what was checked vs not.
//! - **Incomplete (amber)** — timeouts / Unknown on properties that
//!   matter. Frontier-template Unknown is distinguished from per-run
//!   timeout (SPEC §4.3).
//!
//! Four-section artifact (in both `report.md` and `proof.json`):
//! 1. **Proven** — property + deciding solver + tier + source +
//!    template_ref + intent_text.
//! 2. **Refuted** — counterexamples, each as a runnable `forge test`.
//! 3. **Not checked** — skipped catalog templates (with their
//!    activation precondition that failed), frontier templates'
//!    over-approximation, document-only templates,
//!    Phase-5-structural-pending, per-template synthesis failures.
//!    **First-class section; not a footnote.**
//! 4. **Reproduce** — `vergil prove proof.json` command + solver
//!    re-dispatch note.
//!
//! Pure function: `format_verdict(StratifiedInputs) -> VerdictOutput`.
//! No I/O. Slice 8's orchestrator builds the inputs and writes the
//! outputs via `layout::report_md` and `layout::top_level_proof_json`.

use serde::{Deserialize, Serialize};

use vergil_proof::schema::{Source, Tier};

/// Phase 6 Stage 0 fingerprint summary the verdict header echoes.
/// Owned + flat so the formatter is pure: Slice 8 projects the real
/// `vergil_core::fingerprint::Fingerprint` into this shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FingerprintSummary {
    pub interfaces: Vec<String>,
    pub primitives: Vec<String>,
    pub available_oracles: AvailableOraclesSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AvailableOraclesSummary {
    pub tests: bool,
    pub natspec: bool,
    pub readme: bool,
}

/// One verified / refuted / unknown property in the stratified
/// verdict. The shape mirrors `vergil_proof::schema::VerifiedProperty`
/// but adds the verdict variant (V1's schema is Verified-only — Phase
/// 6's verdict UI needs to surface refutations and unknowns too).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyOutcome {
    pub name: String,
    pub source: Source,
    pub tier: Tier,
    pub verdict: PropertyVerdict,
    /// Template id when the property came from the attack catalog.
    /// `None` for tests / natspec / structural / user-intent sources.
    #[serde(default)]
    pub template_ref: Option<String>,
    /// Single English sentence the LLM extracted from the source
    /// (test / NatSpec / catalog negation_property / user input).
    /// Surfaced in the report so the reader sees what the property
    /// *is* without reading the Halmos source.
    #[serde(default)]
    pub intent_text: Option<String>,
}

/// Per-property verdict, richer than V1's "verified or absent" model.
/// SPEC §3.6 wants the verdict UI to distinguish refuted, unknown
/// (with the frontier-vs-timeout distinction per SPEC §4.3), and
/// error states explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PropertyVerdict {
    Verified {
        backend: String,
        #[serde(default)]
        smt_query_sha256: Option<String>,
    },
    Refuted {
        backend: String,
        /// Path relative to project root, e.g.
        /// `vergil-out/counterexamples/Cex_check_transferFrom_blocks_unauthorized.t.sol`.
        cex_file: String,
        #[serde(default)]
        trace_summary: String,
    },
    Unknown {
        detail: String,
        /// Set when the underlying template was frontier-decidable —
        /// surfaces the declared over-approximation so the reader
        /// knows the Unknown is structural to the property, not a
        /// per-run timeout. SPEC §4.3.
        #[serde(default)]
        frontier_over_approximation: Option<String>,
    },
    Error {
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedTemplateSummary {
    pub id: String,
    /// Human-readable activation-failure reason ("no overlap with
    /// required interfaces [ERC4626]" / "pattern X=true not
    /// satisfied"). Comes from the activation engine.
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerTemplateFailureSummary {
    pub template_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentOnlyTemplate {
    pub id: String,
    pub name: String,
}

/// V1.5 Phase 5 — a structural miner finding with confidence below the
/// `cfg.min_confidence` threshold. NOT submitted to the solver; the
/// verdict surfaces it under "Suggested additional invariants" so the
/// user knows the miner had a hunch worth manual review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LowConfidenceStructuralSummary {
    /// Miner that produced this finding (e.g. `"invariant-constants"`).
    pub miner: String,
    pub description: String,
    /// Stringified `"0.55"` etc. — kept as String for snapshot stability.
    pub confidence: String,
    pub fn_or_var: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StratifiedInputs {
    /// Absolute project path. Reproduce section emits a
    /// `vergil prove <project>/vergil-out/proof.json` command using this.
    pub project_path: String,
    pub fingerprint: FingerprintSummary,
    pub properties: Vec<PropertyOutcome>,
    pub skipped_templates: Vec<SkippedTemplateSummary>,
    pub per_template_failures: Vec<PerTemplateFailureSummary>,
    pub document_only_templates: Vec<DocumentOnlyTemplate>,
    /// Phase 5 (structural mining) NOOP marker. Set to `true` when the
    /// structural oracle either didn't run or produced zero candidates;
    /// in that case the "Not checked" section names structural mining
    /// as a deferred oracle. Set to `false` once Phase 5 emits ≥1
    /// candidate — the "Not checked" placeholder is then suppressed
    /// because the user can see the structural-source candidates
    /// directly in §1 Proven / §2 Refuted.
    pub phase5_structural_pending: bool,
    /// V1.5 Phase 5 — below-threshold structural findings, surfaced in
    /// the report's "Suggested additional invariants" section. Empty
    /// when the miner produced no low-confidence findings (the
    /// pre-Phase-5 default, preserved via `#[serde(default)]` so prior
    /// proof artifacts deserialize cleanly).
    #[serde(default)]
    pub low_confidence_structural: Vec<LowConfidenceStructuralSummary>,
}

/// Phase 6 three-state headline. SPEC §3.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Headline {
    Refuted,
    VerifiedInScope,
    Incomplete,
}

impl Headline {
    pub fn as_str(&self) -> &'static str {
        match self {
            Headline::Refuted => "Refuted",
            Headline::VerifiedInScope => "Verified-in-scope",
            Headline::Incomplete => "Incomplete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerdictOutput {
    pub headline: Headline,
    pub inputs: StratifiedInputs,
}

impl VerdictOutput {
    /// Markdown report — written to `vergil-out/report.md`. Four
    /// sections in fixed order; "Not checked" appears even when
    /// every oracle returned a verdict (with an explicit note).
    pub fn report_md(&self) -> String {
        let mut out = String::new();
        let v = &self.inputs;

        // Header + headline.
        out.push_str("# Vergil verdict\n\n");
        out.push_str(&format!("**Headline:** {}\n\n", self.headline.as_str()));
        out.push_str(&format!("Project: `{}`\n", v.project_path));
        out.push_str(&format!(
            "Interfaces: {}\n",
            join_or_none(&v.fingerprint.interfaces)
        ));
        out.push_str(&format!(
            "Primitives: {}\n",
            join_or_none(&v.fingerprint.primitives)
        ));
        out.push_str(&format!(
            "Oracles available: tests={} natspec={} readme={}\n\n",
            v.fingerprint.available_oracles.tests,
            v.fingerprint.available_oracles.natspec,
            v.fingerprint.available_oracles.readme,
        ));

        // §1 Proven
        let proven: Vec<&PropertyOutcome> = v
            .properties
            .iter()
            .filter(|p| matches!(p.verdict, PropertyVerdict::Verified { .. }))
            .collect();
        out.push_str(&format!("## Proven ({})\n\n", proven.len()));
        if proven.is_empty() {
            out.push_str("_No properties verified in this run._\n\n");
        } else {
            for p in proven {
                render_proven_property(&mut out, p);
            }
        }

        // §2 Refuted
        let refuted: Vec<&PropertyOutcome> = v
            .properties
            .iter()
            .filter(|p| matches!(p.verdict, PropertyVerdict::Refuted { .. }))
            .collect();
        out.push_str(&format!("## Refuted ({})\n\n", refuted.len()));
        if refuted.is_empty() {
            out.push_str("_No counterexamples found._\n\n");
        } else {
            for p in refuted {
                render_refuted_property(&mut out, p);
            }
        }

        // V1.5 Phase 5 — Suggested additional invariants (below-
        // threshold structural findings). Skipped when empty so the
        // pre-Phase-5 report layout is preserved.
        if !v.low_confidence_structural.is_empty() {
            out.push_str(&format!(
                "## Suggested additional invariants ({})\n\n",
                v.low_confidence_structural.len()
            ));
            out.push_str(
                "_Mined by structural analysis at low confidence; \
                 NOT auto-verified. Review and promote to a manual \
                 intent if you want them in the next run._\n\n",
            );
            for f in &v.low_confidence_structural {
                let target = f
                    .fn_or_var
                    .as_deref()
                    .map(|t| format!(" `{t}`"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "- **{}** (confidence {}){target}: {}\n",
                    f.miner, f.confidence, f.description,
                ));
            }
            out.push('\n');
        }

        // §3 Not checked — ALWAYS present
        out.push_str("## Not checked\n\n");
        render_not_checked(&mut out, v, &self.headline);

        // §4 Reproduce
        out.push_str("## Reproduce\n\n");
        out.push_str(&format!(
            "Re-verify independently via `vergil prove {}/vergil-out/proof.json`. \
             SMT queries are re-dispatched through the alternate solver (cvc5 by default; \
             override with `--solver z3` / `--solver bitwuzla`). No LLM is invoked on \
             the re-check path.\n",
            v.project_path
        ));
        out
    }

    /// Machine-readable verdict. Same data shape as `report.md`,
    /// JSON-encoded. Slice 8's orchestrator merges this into the
    /// top-level `proof.json` as a `verdict` field; downstream
    /// consumers (Phase 7 bench, future SaaS UI) read this directly.
    pub fn proof_json(&self) -> serde_json::Value {
        serde_json::json!({
            "headline": self.headline.as_str(),
            "headline_machine": match self.headline {
                Headline::Refuted => "refuted",
                Headline::VerifiedInScope => "verified-in-scope",
                Headline::Incomplete => "incomplete",
            },
            "project_path": self.inputs.project_path,
            "fingerprint": self.inputs.fingerprint,
            "properties": self.inputs.properties,
            "skipped_templates": self.inputs.skipped_templates,
            "per_template_failures": self.inputs.per_template_failures,
            "document_only_templates": self.inputs.document_only_templates,
            "phase5_structural_pending": self.inputs.phase5_structural_pending,
            "low_confidence_structural": self.inputs.low_confidence_structural,
            "reproduce": format!(
                "vergil prove {}/vergil-out/proof.json",
                self.inputs.project_path
            ),
        })
    }
}

/// Compute the three-state headline from the property outcomes.
/// Refuted wins (any cex → Refuted); else any non-verified property
/// (Unknown / Error) → Incomplete; else Verified-in-scope.
///
/// "Verified-in-scope" is the honest green: it does NOT mean the
/// contract is safe, only that every property the run chose to check
/// returned UNSAT-on-negation from the solver. The "Not checked"
/// section names everything else.
pub fn format_verdict(inputs: StratifiedInputs) -> VerdictOutput {
    let any_refuted = inputs
        .properties
        .iter()
        .any(|p| matches!(p.verdict, PropertyVerdict::Refuted { .. }));
    let any_unknown_or_error = inputs.properties.iter().any(|p| {
        matches!(
            p.verdict,
            PropertyVerdict::Unknown { .. } | PropertyVerdict::Error { .. }
        )
    });
    let any_verified = inputs
        .properties
        .iter()
        .any(|p| matches!(p.verdict, PropertyVerdict::Verified { .. }));
    let headline = if any_refuted {
        Headline::Refuted
    } else if any_unknown_or_error {
        Headline::Incomplete
    } else if any_verified {
        Headline::VerifiedInScope
    } else {
        // No properties ran at all. Honest read: nothing was actually
        // checked; "Verified-in-scope" would be a lie. Incomplete.
        Headline::Incomplete
    };
    VerdictOutput { headline, inputs }
}

// ─── Markdown helpers ────────────────────────────────────────────────────────

fn join_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "(none)".to_string()
    } else {
        items.join(", ")
    }
}

fn source_label(s: Source) -> &'static str {
    match s {
        Source::UserIntent => "user-intent",
        Source::AttackCatalog => "attack-catalog",
        Source::Conformance => "conformance",
        Source::Tests => "tests",
        Source::NatSpec => "natspec",
        Source::Structural => "structural",
    }
}

fn tier_label(t: Tier) -> &'static str {
    match t {
        Tier::ZeroConfig => "zero-config",
        Tier::Intent => "intent",
    }
}

fn render_proven_property(out: &mut String, p: &PropertyOutcome) {
    let (backend, smt) = match &p.verdict {
        PropertyVerdict::Verified {
            backend,
            smt_query_sha256,
        } => (backend.as_str(), smt_query_sha256.as_deref()),
        _ => unreachable!("render_proven_property called on non-Verified"),
    };
    out.push_str(&format!(
        "- `{}` — tier: {}, source: {}, backend: {}",
        p.name,
        tier_label(p.tier),
        source_label(p.source),
        backend,
    ));
    if let Some(t) = &p.template_ref {
        out.push_str(&format!(", template: `{t}`"));
    }
    if let Some(sha) = smt {
        out.push_str(&format!(", smt: `{}…`", &sha[..sha.len().min(12)]));
    }
    out.push('\n');
    if let Some(intent) = &p.intent_text {
        out.push_str(&format!("    > {intent}\n"));
    }
}

fn render_refuted_property(out: &mut String, p: &PropertyOutcome) {
    let (backend, cex_file, trace) = match &p.verdict {
        PropertyVerdict::Refuted {
            backend,
            cex_file,
            trace_summary,
        } => (backend.as_str(), cex_file.as_str(), trace_summary.as_str()),
        _ => unreachable!("render_refuted_property called on non-Refuted"),
    };
    out.push_str(&format!(
        "- `{}` — tier: {}, source: {}, backend: {}",
        p.name,
        tier_label(p.tier),
        source_label(p.source),
        backend,
    ));
    if let Some(t) = &p.template_ref {
        out.push_str(&format!(", template: `{t}`"));
    }
    out.push('\n');
    if let Some(intent) = &p.intent_text {
        out.push_str(&format!("    > {intent}\n"));
    }
    out.push_str(&format!(
        "    - Counterexample: `{cex_file}` (runnable `forge test --match-path` target)\n"
    ));
    if !trace.is_empty() {
        out.push_str(&format!("    - Trace: {trace}\n"));
    }
}

fn render_not_checked(out: &mut String, v: &StratifiedInputs, headline: &Headline) {
    let mut any = false;

    // Structural mining (Phase 5 deferred — Phase 6 ships without it).
    if v.phase5_structural_pending {
        out.push_str(
            "- **Structural mining (Phase 5)** — not yet implemented. \
             The V1.5 stack will mine conservation / monotonicity / \
             access-policy / invariant-constant / two-step-pattern \
             candidates from solc + Slither once Phase 5 lands.\n",
        );
        any = true;
    }

    // Skipped catalog templates (didn't activate against the project).
    if !v.skipped_templates.is_empty() {
        out.push_str(&format!(
            "- **{} attack-catalog templates skipped** — applicability preconditions did not match:\n",
            v.skipped_templates.len()
        ));
        for s in &v.skipped_templates {
            out.push_str(&format!("    - `{}` — {}\n", s.id, s.reason));
        }
        any = true;
    }

    // Frontier templates with Unknown — surface their over-approximation.
    let frontier_unknowns: Vec<&PropertyOutcome> = v
        .properties
        .iter()
        .filter(|p| {
            matches!(
                &p.verdict,
                PropertyVerdict::Unknown {
                    frontier_over_approximation: Some(_),
                    ..
                }
            )
        })
        .collect();
    if !frontier_unknowns.is_empty() {
        out.push_str(&format!(
            "- **{} frontier-template Unknowns** — the property is fundamentally \
             hard for the underlying SMT theory; the run only verified the declared \
             over-approximation. SPEC §4.3.\n",
            frontier_unknowns.len()
        ));
        for p in frontier_unknowns {
            if let PropertyVerdict::Unknown {
                frontier_over_approximation: Some(approx),
                ..
            } = &p.verdict
            {
                out.push_str(&format!(
                    "    - `{}` (template: `{}`)\n      Over-approximation: {}\n",
                    p.name,
                    p.template_ref.as_deref().unwrap_or("(none)"),
                    approx
                ));
            }
        }
        any = true;
    }

    // Per-run timeouts / errors (distinct from frontier Unknowns above).
    let unknowns: Vec<&PropertyOutcome> = v
        .properties
        .iter()
        .filter(|p| {
            matches!(
                &p.verdict,
                PropertyVerdict::Unknown {
                    frontier_over_approximation: None,
                    ..
                } | PropertyVerdict::Error { .. }
            )
        })
        .collect();
    if !unknowns.is_empty() {
        out.push_str(&format!(
            "- **{} per-run Unknowns or errors** — solver budget exhausted or \
             dispatch failure, not a frontier-template structural issue (SPEC §4.3 \
             distinction):\n",
            unknowns.len()
        ));
        for p in unknowns {
            let detail = match &p.verdict {
                PropertyVerdict::Unknown { detail, .. } => detail.as_str(),
                PropertyVerdict::Error { detail } => detail.as_str(),
                _ => "",
            };
            out.push_str(&format!("    - `{}` — {}\n", p.name, detail));
        }
        any = true;
    }

    // Document-only templates (smt_status = document_only — no encoding).
    if !v.document_only_templates.is_empty() {
        out.push_str(&format!(
            "- **{} document-only templates** — no SMT encoding ships; these are \
             named in the catalog for awareness only.\n",
            v.document_only_templates.len()
        ));
        for d in &v.document_only_templates {
            out.push_str(&format!("    - `{}` — {}\n", d.id, d.name));
        }
        any = true;
    }

    // Per-template synthesis failures (catalog-as-oracle couldn't land
    // a Halmos check_ function).
    if !v.per_template_failures.is_empty() {
        out.push_str(&format!(
            "- **{} attack-catalog templates failed synthesis** — the LLM could \
             not render a check_ function for the template against this contract \
             surface:\n",
            v.per_template_failures.len()
        ));
        for f in &v.per_template_failures {
            out.push_str(&format!("    - `{}` — {}\n", f.template_id, f.reason));
        }
        any = true;
    }

    if !any {
        // SPEC §3.6: the section is always present, even when empty.
        out.push_str(
            "_All selected oracles produced verdicts; no skipped templates, \
             frontier Unknowns, or deferred sources to report._\n",
        );
    }

    // Headline-specific call-out: Verified-in-scope must restate
    // scope explicitly so the user doesn't read "Verified-in-scope"
    // as "safe".
    if matches!(headline, Headline::VerifiedInScope) {
        out.push_str(
            "\n_The \"Verified-in-scope\" headline above means every property this \
             run CHECKED was proven by the solver. It does NOT mean the contract is \
             safe. Properties listed above as skipped / Phase-5-pending / \
             document-only were not checked at all._\n",
        );
    }

    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_inputs(project: &str) -> StratifiedInputs {
        StratifiedInputs {
            project_path: project.to_string(),
            fingerprint: FingerprintSummary::default(),
            properties: Vec::new(),
            skipped_templates: Vec::new(),
            per_template_failures: Vec::new(),
            document_only_templates: Vec::new(),
            phase5_structural_pending: true,
            low_confidence_structural: Vec::new(),
        }
    }

    fn verified(name: &str, src: Source, tier: Tier) -> PropertyOutcome {
        PropertyOutcome {
            name: name.to_string(),
            source: src,
            tier,
            verdict: PropertyVerdict::Verified {
                backend: "halmos".to_string(),
                smt_query_sha256: Some("a".repeat(64)),
            },
            template_ref: None,
            intent_text: Some(format!("English intent for {name}")),
        }
    }

    fn refuted_catalog(name: &str, attack_id: &str) -> PropertyOutcome {
        PropertyOutcome {
            name: name.to_string(),
            source: Source::AttackCatalog,
            tier: Tier::ZeroConfig,
            verdict: PropertyVerdict::Refuted {
                backend: "halmos".to_string(),
                cex_file: format!("vergil-out/counterexamples/Cex_{name}.t.sol"),
                trace_summary: "attacker mints supply without authorization".to_string(),
            },
            template_ref: Some(attack_id.to_string()),
            intent_text: Some(
                "Only the owner can mint; unauthorized callers cannot inflate supply.".to_string(),
            ),
        }
    }

    fn frontier_unknown(name: &str, attack_id: &str, approx: &str) -> PropertyOutcome {
        PropertyOutcome {
            name: name.to_string(),
            source: Source::AttackCatalog,
            tier: Tier::ZeroConfig,
            verdict: PropertyVerdict::Unknown {
                detail: "frontier template; over-approximation verified only".to_string(),
                frontier_over_approximation: Some(approx.to_string()),
            },
            template_ref: Some(attack_id.to_string()),
            intent_text: None,
        }
    }

    // ─── Acceptance 1: three-state headline on each canned case ──────────

    #[test]
    fn headline_refuted_when_any_property_is_refuted() {
        let mut inputs = empty_inputs("/p");
        inputs.properties = vec![
            verified("check_a", Source::Tests, Tier::ZeroConfig),
            refuted_catalog("check_b", "access-public-burn-mint"),
        ];
        let out = format_verdict(inputs);
        assert_eq!(out.headline, Headline::Refuted);
    }

    #[test]
    fn headline_verified_in_scope_when_all_properties_verified_and_none_unknown() {
        let mut inputs = empty_inputs("/p");
        inputs.properties = vec![
            verified("check_a", Source::AttackCatalog, Tier::ZeroConfig),
            verified("check_b", Source::Tests, Tier::ZeroConfig),
            verified("check_c", Source::NatSpec, Tier::ZeroConfig),
        ];
        let out = format_verdict(inputs);
        assert_eq!(out.headline, Headline::VerifiedInScope);
    }

    #[test]
    fn headline_incomplete_when_any_property_unknown_or_error() {
        let mut inputs = empty_inputs("/p");
        inputs.properties = vec![
            verified("check_a", Source::Tests, Tier::ZeroConfig),
            frontier_unknown("check_b", "lending-solvency-frontier", "treat oracle as opaque"),
        ];
        let out = format_verdict(inputs);
        assert_eq!(out.headline, Headline::Incomplete);
    }

    #[test]
    fn headline_incomplete_when_no_properties_ran() {
        // Empty oracles. "Verified-in-scope" would be dishonest —
        // there's nothing to be in-scope. Incomplete.
        let out = format_verdict(empty_inputs("/p"));
        assert_eq!(out.headline, Headline::Incomplete);
    }

    // ─── Acceptance 2: "Not checked" section always present ──────────────

    #[test]
    fn not_checked_section_always_appears_even_when_oracles_complete() {
        let mut inputs = empty_inputs("/p");
        inputs.phase5_structural_pending = false; // pretend Phase 5 landed
        inputs.properties = vec![verified("check_a", Source::Tests, Tier::ZeroConfig)];
        let report = format_verdict(inputs).report_md();
        assert!(
            report.contains("## Not checked"),
            "Not checked section must always be present"
        );
        assert!(
            report.contains("All selected oracles produced verdicts"),
            "empty Not-checked section must include the explicit note: {report}"
        );
    }

    #[test]
    fn not_checked_section_lists_phase5_when_pending() {
        let inputs = empty_inputs("/p");
        let report = format_verdict(inputs).report_md();
        assert!(
            report.contains("Structural mining (Phase 5)"),
            "Phase 5 pending marker missing: {report}"
        );
    }

    #[test]
    fn low_confidence_structural_section_renders_when_present() {
        // V1.5 Phase 5 Slice 6 — when the structural miner emits
        // below-threshold findings, the verdict adds a "Suggested
        // additional invariants" section between Refuted and Not-checked.
        let mut inputs = empty_inputs("/p");
        inputs.phase5_structural_pending = false;
        inputs.low_confidence_structural = vec![
            LowConfidenceStructuralSummary {
                miner: "invariant-constants".into(),
                description: "totalSupply written only in constructor; \
                    value depends on constructor arg — verify invariance manually"
                    .into(),
                confidence: "0.55".into(),
                fn_or_var: Some("totalSupply".into()),
            },
        ];
        let report = format_verdict(inputs).report_md();
        assert!(
            report.contains("## Suggested additional invariants (1)"),
            "missing low-confidence header: {report}"
        );
        assert!(report.contains("NOT auto-verified"));
        assert!(report.contains("invariant-constants"));
        assert!(report.contains("confidence 0.55"));
        assert!(report.contains("`totalSupply`"));
        assert!(report.contains("verify invariance manually"));
    }

    #[test]
    fn low_confidence_structural_section_absent_when_empty() {
        // Empty findings → section is skipped (preserves pre-Phase-5
        // report layout for older runs / mock-LLM tests).
        let inputs = empty_inputs("/p");
        let report = format_verdict(inputs).report_md();
        assert!(
            !report.contains("Suggested additional invariants"),
            "section should not appear when empty: {report}"
        );
    }

    #[test]
    fn phase5_placeholder_suppressed_when_oracle_emitted_candidates() {
        // V1.5 Phase 5 Slice 6 — once phase5_structural_pending=false
        // the "Structural mining (Phase 5) — not yet implemented"
        // marker MUST NOT appear in Not-checked. The structural
        // candidates themselves appear in §1 Proven / §2 Refuted.
        let mut inputs = empty_inputs("/p");
        inputs.phase5_structural_pending = false;
        inputs.properties = vec![verified("check_invariant_DECIMALS", Source::Structural, Tier::ZeroConfig)];
        let report = format_verdict(inputs).report_md();
        assert!(
            !report.contains("Structural mining (Phase 5)"),
            "placeholder must disappear once oracle emits: {report}"
        );
        // Verify the property does appear under Proven.
        assert!(report.contains("source: structural"), "missing structural property: {report}");
    }

    #[test]
    fn not_checked_section_lists_skipped_templates_with_reasons() {
        let mut inputs = empty_inputs("/p");
        inputs.skipped_templates = vec![SkippedTemplateSummary {
            id: "vault-inflation-first-depositor-donation".to_string(),
            reason: "no overlap with required interfaces [ERC4626]".to_string(),
        }];
        let report = format_verdict(inputs).report_md();
        assert!(report.contains("vault-inflation-first-depositor-donation"));
        assert!(report.contains("no overlap with required interfaces"));
    }

    #[test]
    fn not_checked_section_distinguishes_frontier_unknowns_from_runtime_unknowns() {
        let mut inputs = empty_inputs("/p");
        inputs.properties = vec![
            frontier_unknown("check_a", "lending-solvency", "treat oracle as opaque"),
            PropertyOutcome {
                name: "check_b".to_string(),
                source: Source::AttackCatalog,
                tier: Tier::ZeroConfig,
                verdict: PropertyVerdict::Unknown {
                    detail: "Halmos timed out at 60s".to_string(),
                    frontier_over_approximation: None,
                },
                template_ref: None,
                intent_text: None,
            },
        ];
        let report = format_verdict(inputs).report_md();
        assert!(report.contains("frontier-template Unknowns"));
        assert!(report.contains("treat oracle as opaque"));
        assert!(report.contains("per-run Unknowns or errors"));
        assert!(report.contains("Halmos timed out"));
    }

    #[test]
    fn verified_in_scope_includes_safe_is_not_safe_disclaimer() {
        let mut inputs = empty_inputs("/p");
        inputs.properties = vec![verified("check_a", Source::AttackCatalog, Tier::ZeroConfig)];
        let report = format_verdict(inputs).report_md();
        assert!(report.contains("does NOT mean the contract is safe"));
    }

    // ─── Acceptance 3: Reproduce section emits a runnable command ────────

    #[test]
    fn reproduce_section_emits_runnable_vergil_prove_command() {
        let inputs = empty_inputs("/path/to/project");
        let report = format_verdict(inputs).report_md();
        assert!(
            report.contains("vergil prove /path/to/project/vergil-out/proof.json"),
            "Reproduce command missing or wrong: {report}"
        );
    }

    // ─── Acceptance 4: markdown and JSON outputs carry same data ─────────

    #[test]
    fn proof_json_carries_same_headline_as_report_md() {
        let mut inputs = empty_inputs("/p");
        inputs.properties = vec![refuted_catalog("check_a", "access-public-burn-mint")];
        let out = format_verdict(inputs);
        let json = out.proof_json();
        assert_eq!(json["headline"], "Refuted");
        assert_eq!(json["headline_machine"], "refuted");
        assert!(out.report_md().contains("**Headline:** Refuted"));
    }

    #[test]
    fn proof_json_includes_all_inputs() {
        let mut inputs = empty_inputs("/p");
        inputs.properties = vec![verified("check_a", Source::Tests, Tier::ZeroConfig)];
        inputs.skipped_templates = vec![SkippedTemplateSummary {
            id: "foo".to_string(),
            reason: "no match".to_string(),
        }];
        let out = format_verdict(inputs.clone());
        let json = out.proof_json();
        assert_eq!(json["properties"].as_array().unwrap().len(), 1);
        assert_eq!(json["skipped_templates"].as_array().unwrap().len(), 1);
        assert!(json["reproduce"]
            .as_str()
            .unwrap()
            .contains("vergil prove /p/vergil-out/proof.json"));
    }

    // ─── Property-section rendering ──────────────────────────────────────

    #[test]
    fn proven_section_lists_each_verified_property_with_provenance() {
        let mut inputs = empty_inputs("/p");
        inputs.properties = vec![
            verified("check_a", Source::AttackCatalog, Tier::ZeroConfig),
            verified("check_b", Source::Tests, Tier::ZeroConfig),
        ];
        let report = format_verdict(inputs).report_md();
        assert!(report.contains("## Proven (2)"));
        assert!(report.contains("source: attack-catalog"));
        assert!(report.contains("source: tests"));
        assert!(report.contains("English intent for check_a"));
    }

    #[test]
    fn refuted_section_lists_cex_file_path() {
        let mut inputs = empty_inputs("/p");
        inputs.properties = vec![refuted_catalog("check_a", "access-public-burn-mint")];
        let report = format_verdict(inputs).report_md();
        assert!(report.contains("## Refuted (1)"));
        assert!(report.contains("vergil-out/counterexamples/Cex_check_a.t.sol"));
        assert!(report.contains("attacker mints supply"));
    }

    #[test]
    fn proven_section_announces_empty_when_no_verified() {
        let mut inputs = empty_inputs("/p");
        inputs.properties = vec![refuted_catalog("check_a", "x")];
        let report = format_verdict(inputs).report_md();
        assert!(report.contains("## Proven (0)"));
        assert!(report.contains("No properties verified in this run"));
    }
}
