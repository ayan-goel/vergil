//! CEGIS outer loop: the closed-loop orchestration that ties synthesis,
//! critique, mutation testing, manifest validation, portfolio dispatch,
//! diagnosis, and refinement into a single iterative pipeline.
//!
//! Per SPEC §3.1 / §11.2: hard cap of 10 iterations. Per-iteration state
//! tracked in [`CegisState`] for resumability and Pareto frontier analysis.
//! Cost telemetry surfaces per iteration so a runaway loop can be
//! hard-stopped via the configurable `cost_budget_usd` cap.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::critique::{Critic, CritiqueResult};
use crate::diagnosis::{Diagnosis, DiagnosisClass, Diagnostician};
use crate::refinement::{CodeRepairPlan, RefinedSpec, Refiner};
use crate::synthesis::{
    synthesize, RetrievedHint, SampleStat, SpecCandidate, StaticAnalysisSummary, SynthesisConfig,
    SynthesisError,
};
use vergil_llm::LlmProvider;

/// Configuration for one full CEGIS run.
#[derive(Debug, Clone)]
pub struct CegisConfig {
    /// Hard ceiling on outer iterations (SPEC §3.1 default: 10).
    pub max_iterations: usize,
    pub synthesis: SynthesisConfig,
    /// Production runtime cost cap. Slice 13 step 5: $200 prod default,
    /// $1-$2 in dev tests, $5-$10 per contract in kill-criterion runner.
    pub cost_budget_usd: f64,
    /// Cost-per-million-token estimates used for the soft budget check.
    /// Empirical; tune per provider.
    pub cost_per_mtok_in_usd: f64,
    pub cost_per_mtok_out_usd: f64,
}

impl Default for CegisConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            synthesis: SynthesisConfig::default_for_anthropic(),
            cost_budget_usd: 200.0,
            // Claude Opus 4.7 ballpark — recalibrate when models change.
            cost_per_mtok_in_usd: 15.0,
            cost_per_mtok_out_usd: 75.0,
        }
    }
}

impl CegisConfig {
    pub fn for_tests() -> Self {
        Self {
            max_iterations: 3,
            synthesis: SynthesisConfig::for_tests(),
            cost_budget_usd: 1.0,
            cost_per_mtok_in_usd: 15.0,
            cost_per_mtok_out_usd: 75.0,
        }
    }
}

/// Per-spec result from a single CEGIS iteration. Persisted to
/// `vergil-out/loop-state.json` (Slice 14 will serialize this).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateOutcome {
    pub candidate: SpecCandidate,
    pub critique: Option<CritiqueResult>,
    pub mutation_coverage: Option<f64>,
    pub manifest_warnings: Vec<String>,
    pub verifier_verdict: VerifierVerdict,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VerifierVerdict {
    NotRun,
    Verified {
        /// Which backend produced the verdict. Lowercase, stable identifier.
        #[serde(default = "default_backend_label")]
        backend: String,
        /// SHA-256 of the SMT-LIB query captured from the winning backend
        /// (Halmos `--dump-smt-queries` / SMTChecker `--model-checker-print-query`).
        /// `None` when SMT capture wasn't enabled or the backend didn't dump.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        smt_query_sha256: Option<String>,
    },
    Counterexample {
        message: String,
    },
    Unknown {
        detail: String,
    },
    Error {
        detail: String,
    },
}

fn default_backend_label() -> String {
    "halmos".to_string()
}

impl VerifierVerdict {
    /// Convenience constructor for the common "verified without SMT capture"
    /// case. Tests use this; production code paths thread the hash through.
    pub fn verified() -> Self {
        VerifierVerdict::Verified {
            backend: default_backend_label(),
            smt_query_sha256: None,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct IterationStats {
    pub iteration: usize,
    pub synthesized: usize,
    pub dropped_critique: usize,
    pub dropped_mutation: usize,
    pub dropped_manifest: usize,
    pub dispatched: usize,
    pub verified: usize,
    pub counterexamples: usize,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub wall_clock_ms: u64,
    pub diagnosis: Option<DiagnosisClass>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CegisRun {
    pub iterations: Vec<IterationStats>,
    pub outcomes: Vec<CandidateOutcome>,
    pub total_cost_usd: f64,
    pub stop_reason: Option<String>,
    pub repaired_code: Option<CodeRepairPlan>,
    pub refined_specs: Vec<RefinedSpec>,
    pub final_diagnosis: Option<Diagnosis>,
}

#[derive(Debug, thiserror::Error)]
pub enum CegisError {
    #[error("synthesis: {0}")]
    Synthesis(#[from] SynthesisError),
}

/// Glue trait the orchestrator delegates verifier dispatch to. Production
/// implementation lives in Slice 14 / `vergil-cli` and routes through
/// `vergil-core::portfolio::dispatch`. Keeping this trait makes the loop
/// testable end-to-end without a real Foundry project.
#[async_trait::async_trait]
pub trait VerifierDispatcher: Send + Sync {
    async fn dispatch(&self, spec: &SpecCandidate) -> VerifierVerdict;
}

/// Optional mutation-scorer indirection. The loop calls it when present
/// and accepts coverage = 1.0 (i.e. skip the 0.4 gate) when None, matching
/// the SPEC §11.2 degraded-mode contract.
#[async_trait::async_trait]
pub trait MutationGate: Send + Sync {
    async fn coverage(&self, spec: &SpecCandidate) -> f64;
}

pub struct CegisLoop {
    pub synthesizer: Arc<dyn LlmProvider>,
    pub critic: Critic,
    pub diagnostician: Diagnostician,
    pub refiner: Refiner,
    pub mutation_gate: Option<Arc<dyn MutationGate>>,
    pub dispatcher: Arc<dyn VerifierDispatcher>,
    pub cfg: CegisConfig,
    /// Minimum mutation coverage to keep a candidate. SPEC §3.7 default 0.4.
    pub mutation_min: f64,
}

impl CegisLoop {
    pub async fn run(
        &self,
        intent: &str,
        sa: &StaticAnalysisSummary,
        retrieved: &[RetrievedHint],
        contract_source: &str,
    ) -> Result<CegisRun, CegisError> {
        self.run_with_description(intent, None, "", sa, retrieved, contract_source)
            .await
    }

    /// CEGIS loop variant where the caller passes a property-specific
    /// `description` (in addition to the broader `intent`) — used by the
    /// kill-criterion runner so the critic scores each candidate against the
    /// one ground-truth property the iteration is targeting.
    /// `available_methods` is the synth-prompt block listing the contract's
    /// external/public function signatures (Phase 4 Slice A3). Pass an
    /// empty string and the renderer substitutes a placeholder; callers
    /// should prefer `vergil_solidity::signatures::render_available_methods`.
    pub async fn run_with_description(
        &self,
        intent: &str,
        description: Option<&str>,
        available_methods: &str,
        sa: &StaticAnalysisSummary,
        retrieved: &[RetrievedHint],
        contract_source: &str,
    ) -> Result<CegisRun, CegisError> {
        let mut run = CegisRun::default();
        let mut iteration = 0usize;

        loop {
            if iteration >= self.cfg.max_iterations {
                run.stop_reason = Some(format!("max_iterations ({})", self.cfg.max_iterations));
                break;
            }
            let started = std::time::Instant::now();
            let mut stats = IterationStats {
                iteration,
                ..Default::default()
            };

            // 1. Synthesize k candidates.
            let synth = synthesize(
                self.synthesizer.clone(),
                intent,
                available_methods,
                sa,
                retrieved,
                contract_source,
                &self.cfg.synthesis,
            )
            .await?;
            stats.synthesized = synth.candidates.len();
            for s in &synth.samples {
                self.account_for_sample(&mut stats, s, &mut run);
            }

            // 2. Critique each.
            let critiques = self
                .critic
                .critique_all(&synth.candidates, intent, description)
                .await;
            let critique_outcome = self.critic.filter_accepted(synth.candidates, critiques);
            stats.dropped_critique = critique_outcome.dropped.len();

            // 3. Mutation gate.
            let mut survivors: Vec<(SpecCandidate, CritiqueResult, Option<f64>)> = Vec::new();
            for (c, r) in critique_outcome.kept {
                let coverage = match &self.mutation_gate {
                    Some(g) => Some(g.coverage(&c).await),
                    None => None,
                };
                if let Some(cov) = coverage {
                    if cov < self.mutation_min {
                        stats.dropped_mutation += 1;
                        run.outcomes.push(CandidateOutcome {
                            candidate: c,
                            critique: Some(r),
                            mutation_coverage: Some(cov),
                            manifest_warnings: Vec::new(),
                            verifier_verdict: VerifierVerdict::NotRun,
                        });
                        continue;
                    }
                }
                survivors.push((c, r, coverage));
            }

            // 4. Verifier dispatch.
            for (c, r, cov) in survivors {
                let verdict = self.dispatcher.dispatch(&c).await;
                match &verdict {
                    VerifierVerdict::Verified { .. } => stats.verified += 1,
                    VerifierVerdict::Counterexample { .. } => stats.counterexamples += 1,
                    _ => {}
                }
                stats.dispatched += 1;
                run.outcomes.push(CandidateOutcome {
                    candidate: c,
                    critique: Some(r),
                    mutation_coverage: cov,
                    manifest_warnings: Vec::new(),
                    verifier_verdict: verdict,
                });
            }

            // 5. Decide whether to refine + which way.
            stats.wall_clock_ms = started.elapsed().as_millis() as u64;
            run.iterations.push(stats);

            if run
                .outcomes
                .iter()
                .any(|o| matches!(o.verifier_verdict, VerifierVerdict::Verified { .. }))
            {
                run.stop_reason = Some("verified".to_string());
                break;
            }

            // Pull a counterexample to feed diagnosis (the first one).
            let cex = run
                .outcomes
                .iter()
                .rev()
                .find_map(|o| match &o.verifier_verdict {
                    VerifierVerdict::Counterexample { message } => {
                        Some((o.candidate.clone(), message.clone()))
                    }
                    _ => None,
                });
            let (spec_for_diag, cex_trace) = match cex {
                Some(v) => v,
                None => {
                    run.stop_reason = Some("no_definitive_verdict".to_string());
                    break;
                }
            };

            let diag = self
                .diagnostician
                .diagnose(intent, &spec_source_blob(&spec_for_diag), &cex_trace)
                .await;
            if let Some(last) = run.iterations.last_mut() {
                last.diagnosis = Some(diag.class);
            }
            run.final_diagnosis = Some(diag.clone());

            match diag.class {
                DiagnosisClass::CodeBug => {
                    match self
                        .refiner
                        .repair_code(intent, &spec_for_diag, contract_source, &cex_trace)
                        .await
                    {
                        Ok(plan) => run.repaired_code = Some(plan),
                        Err(e) => {
                            run.stop_reason = Some(format!("repair_code error: {e}"));
                            break;
                        }
                    }
                }
                DiagnosisClass::SpecBug => {
                    match self
                        .refiner
                        .refine_spec(intent, &spec_for_diag, contract_source, &cex_trace)
                        .await
                    {
                        Ok(refined) => run.refined_specs.push(refined),
                        Err(e) => {
                            run.stop_reason = Some(format!("refine_spec error: {e}"));
                            break;
                        }
                    }
                }
                DiagnosisClass::Ambiguous => {
                    run.stop_reason = Some("ambiguous_diagnosis".to_string());
                    break;
                }
            }

            // 6. Cost budget hard-stop.
            if run.total_cost_usd >= self.cfg.cost_budget_usd {
                run.stop_reason = Some(format!(
                    "cost budget ${:.2} reached (limit ${:.2})",
                    run.total_cost_usd, self.cfg.cost_budget_usd
                ));
                break;
            }

            iteration += 1;
        }

        if run.stop_reason.is_none() {
            run.stop_reason = Some("loop_completed".to_string());
        }
        Ok(run)
    }

    fn account_for_sample(&self, stats: &mut IterationStats, s: &SampleStat, run: &mut CegisRun) {
        stats.tokens_in = stats.tokens_in.saturating_add(s.tokens_in);
        stats.tokens_out = stats.tokens_out.saturating_add(s.tokens_out);
        let usd = (s.tokens_in as f64) / 1_000_000.0 * self.cfg.cost_per_mtok_in_usd
            + (s.tokens_out as f64) / 1_000_000.0 * self.cfg.cost_per_mtok_out_usd;
        run.total_cost_usd += usd;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_estimate_matches_sample_tokens() {
        let mut cfg = CegisConfig::for_tests();
        cfg.cost_per_mtok_in_usd = 1.0;
        cfg.cost_per_mtok_out_usd = 2.0;
        let mut stats = IterationStats::default();
        let mut run = CegisRun::default();
        let s = SampleStat {
            index: 0,
            temperature: 0.0,
            tokens_in: 1_000_000,
            tokens_out: 500_000,
            latency_ms: 0,
            candidate_count: 0,
        };
        // Build a stand-in loop just to call account_for_sample.
        struct FakeDispatch;
        #[async_trait::async_trait]
        impl VerifierDispatcher for FakeDispatch {
            async fn dispatch(&self, _spec: &SpecCandidate) -> VerifierVerdict {
                VerifierVerdict::NotRun
            }
        }
        let provider = Arc::new(vergil_llm::mock::MockProvider::new("/tmp/x"));
        let loop_ = CegisLoop {
            synthesizer: provider.clone(),
            critic: Critic::new(
                provider.clone(),
                vergil_llm::ProviderId::Mock,
                crate::critique::CritiqueConfig::for_tests(),
            ),
            diagnostician: Diagnostician::new(
                provider.clone(),
                crate::diagnosis::DiagnosisConfig::for_tests(),
            ),
            refiner: Refiner::new(
                provider.clone(),
                crate::refinement::RefinementConfig::for_tests(),
            ),
            mutation_gate: None,
            dispatcher: Arc::new(FakeDispatch),
            cfg,
            mutation_min: 0.4,
        };
        loop_.account_for_sample(&mut stats, &s, &mut run);
        assert_eq!(stats.tokens_in, 1_000_000);
        assert_eq!(stats.tokens_out, 500_000);
        // 1M in * $1 + 0.5M out * $2 = $1 + $1 = $2
        assert!((run.total_cost_usd - 2.0).abs() < 1e-6);
    }
}
