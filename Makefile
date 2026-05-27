# Vergil Makefile — manual benchmark + integration targets.
#
# Mirrors the workflow_dispatch GitHub Actions so you can run each one
# locally before clicking the UI button. Every cost-incurring target
# prints the estimated dollar cost and waits for `y` confirmation
# before launching.
#
# The Phase 3 rule (per tasks/plan.md): NEVER run these on a schedule.
# Cost is owned — the user decides when to spend.

VERGIL := ./target/release/vergil
VERGILBENCH := ./target/release/vergilbench
KILL_CRITERION := ./target/release/kill-criterion

.PHONY: help build bench kill-criterion llm-live ci-audit

help:
	@echo "Vergil — manual benchmark targets"
	@echo
	@echo "  make build           Build all release binaries (vergil, vergilbench, kill-criterion)"
	@echo "  make bench           Run the VergilBench seed corpus.    Cost: \$$0 (Phase 3 seed)"
	@echo "  make kill-criterion  Run the Phase 2 kill criterion.     Cost: \$$12, ~22 min wall clock"
	@echo "  make llm-live        Run live-API integration tests.     Cost: ~\$$0.05, ~2 min"
	@echo "  make ci-audit        Confirm no schedule: triggers in .github/workflows/"
	@echo
	@echo "Every cost-incurring target prompts [y/N] before launching."

build:
	cargo build --release --bin vergil --bin vergilbench --bin kill-criterion

bench: build
	@echo "VergilBench seed corpus run (5 contracts)."
	@echo "Estimated cost: \$$0 (Phase 1 deterministic path)."
	@echo "Estimated wall clock: ~30 seconds."
	@printf "Continue? [y/N] "
	@read ans && [ "$$ans" = "y" ] || (echo "Aborted."; exit 1)
	$(VERGILBENCH) --corpus vergilbench

kill-criterion: build
	@echo "Phase 2 kill criterion sweep (22 properties)."
	@echo "Estimated cost: \$$12 (Anthropic Sonnet 4.6 synth + OpenAI GPT-5.5 critic)."
	@echo "Estimated wall clock: ~22 minutes."
	@printf "Continue? [y/N] "
	@read ans && [ "$$ans" = "y" ] || (echo "Aborted."; exit 1)
	$(KILL_CRITERION)

llm-live:
	@echo "Live LLM integration tests."
	@echo "Estimated cost: ~\$$0.05."
	@printf "Continue? [y/N] "
	@read ans && [ "$$ans" = "y" ] || (echo "Aborted."; exit 1)
	cargo test --workspace --features llm-live

ci-audit:
	@echo "Auditing .github/workflows/ for schedule: triggers..."
	@! grep -r "^[[:space:]]*schedule:" .github/workflows/ 2>/dev/null \
		|| (echo "FAIL: schedule: trigger found above. Per Phase 3 rule (tasks/plan.md), CI is manual-only."; exit 1)
	@echo "PASS: no schedule: triggers found."
