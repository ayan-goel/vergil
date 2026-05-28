# V2 readiness checklist

**Scope:** what V2 has to wire that Phase 4 didn't. Each item lists the
Phase 4 artifact it builds on and the rough shape of the V2 work.

> **Audience:** the V2 engineer (or you, six months from now) starting
> the hosted-SaaS build. Phase 4 stops where the kernel and the seam
> stop; V2 picks up here.

## 1. Persistence

**Phase 4:** `JobStore` trait + `InMemoryStore` impl + shipped-not-executed
postgres migration (`migrations/0001_jobs.sql`). ADR 0002.

**V2 work:**

- Wire a real postgres backend behind `JobStore`. Likely sqlx or
  diesel; pool tunables go in service config.
- Run the migration on the production DB (and add a `0002_...` for
  every subsequent schema change — no in-place edits).
- Connection-pool sizing per worker pool, retry policy, statement
  timeout.
- Backup + restore play (out of scope for this checklist; talk to
  whoever owns Postgres ops at V2).

## 2. Multi-tenant auth

**Phase 4:** `AuthProvider` trait + `SingleTokenAuth` env-driven stub.
ADR 0004.

**V2 work:**

- Pick the identity provider (Auth0 / Clerk / WorkOS / postgres-backed
  tokens). Likely a combination: human users via OIDC, machine
  integrations via long-lived tokens.
- New `vergil-service/src/auth/<provider>.rs` implementing
  `AuthProvider`. `AuthIdentity.tenant_id` flows the rest of the way
  unchanged.
- Per-tenant scopes / RBAC. 401 (missing) vs 403 (insufficient scope)
  split.
- Token rotation + revocation flows.
- Rate-limiting per token (tower-governor or equivalent).

## 3. Container registry + deployment infrastructure

**Phase 4:** `Dockerfile` + `.dockerignore`. Multi-stage, healthchecked,
builds clean on amd64 + arm64. **Not pushed to a registry.** ADR 0003.

**V2 work:**

- Pick the registry (ECR / GHCR / GAR / private hub).
- CI: workflow to build + push on tags. (Phase 4 CI is manual-only;
  V2 can keep that pattern or add automated tagged builds.)
- Pick the orchestrator: Kubernetes / ECS / Fly.io / Modal / Nomad.
  Each has different bind-mount + secret-management primitives;
  pick once and stop relitigating.
- Worker pool sizing model: how many concurrent verification runs
  per node, given CPU + RAM + disk + per-run cost cap.
- Job queue between the service and the worker pool. Likely
  Redis-backed (RQ-style) or postgres-backed (pg-boss / SKIP LOCKED).

## 4. Subprocess sandbox (carryover from B1)

**Phase 4:** `vergil_solidity::sandbox` primitives — macOS sandbox-exec
+ Linux bubblewrap wrappers. **Not wired into the actual subprocess
call sites** (halmos / solc / forge / slither). ADR not written
separately; see Slice B1's commit message.

**V2 work:**

- Wire `sandbox_command(...)` into:
  - `vergil_solidity::halmos::run_simple` (the main halmos invocation)
  - `vergil_solidity::static_analysis::analyze` (solc + slither)
  - `vergil_solidity::foundry::compile` (forge invocations)
- Tune the bubblewrap bind-mount set against the bench corpus to
  catch any subprocess that needs a path Phase 4 didn't anticipate.
- Add `--no-sandbox` CLI flag for diagnostic use.
- Flip the default to **sandbox-on** once tuned.
- Defense-in-depth: layer bubblewrap inside the worker container so
  even a sandbox escape can't cross the container boundary.

## 5. Telemetry pipeline

**Phase 4:** JSONL events written to a file via `--telemetry-json`;
Prometheus stub at `GET /metrics`; tracing spans on the CEGIS loop.
ADR 0005.

**V2 work:**

- Replace `JsonlSink` with a streaming sink that pushes to V2's event
  bus (Kafka / Kinesis / Redis Streams). The `TelemetrySink` trait
  is the seam.
- Wire `tracing_opentelemetry::layer()` into the subscriber and ship
  spans to the observability backend (Honeycomb / Datadog / Tempo).
- Wire `state.metrics.inc(...)` calls in the service handlers
  (`submit_job` increments `vergil_jobs_total{status="pending"}`,
  status transitions update the counters, telemetry ingest increments
  `vergil_telemetry_events_total`).
- Per-tenant cost rollup: aggregate JSONL events into the
  `tenant_cost_monthly` view (schema ships in ADR 0002).

## 6. Billing integration

**Phase 4:** zero billing infrastructure. Cost is tracked per run via
the telemetry stream; nothing aggregates or invoices.

**V2 work:**

- Pick the billing provider (Stripe is the obvious default).
- Cron / scheduled job that reads `tenant_cost_monthly` and pushes
  invoices.
- Usage-based pricing model: $X per verification + $Y per dollar of
  LLM spend passed through, or some flat tier + overage. Pricing
  decision lives outside this doc.
- Customer billing portal (likely Stripe-hosted in V2; custom dashboard
  later).

## 7. Customer-facing UI

**Phase 4:** zero UI. The CLI + the OpenAPI contract are the entirety
of the surface.

**V2 work:**

- Pick the form factor: web dashboard, Foundry plugin, GitHub App,
  or some combination. Likely all three eventually.
- Web dashboard: React or Solid, talks directly to the V1 API. The
  Phase 4 `openapi.yaml` generates a typed TypeScript client.
- Foundry plugin: wraps `vergil verify` for in-editor use during dev.
- GitHub App: runs verification on PR open, comments with the proof.
  Auth is the app-installation token; per-org tenant ID.

## 8. Onboarding flow

**Phase 4:** zero onboarding. The CLI assumes you already have API
keys + Foundry + your contract source.

**V2 work:**

- Signup → email verify → workspace setup → first integration token.
- Sample contracts + sample intents in the dashboard so new accounts
  can run a verification without prep.
- Integration docs for each form factor (CLI / GitHub App / dashboard).

## 9. Customer support tooling

**Phase 4:** zero support tooling. Internal team can grep the JSONL
trace; that's it.

**V2 work:**

- Admin dashboard: per-tenant cost, per-tenant runs, per-tenant error
  rates.
- Trace viewer: hand a support engineer a run ID, get the full CEGIS
  loop transcript (synth samples → critique scores → dispatch results
  → cost).
- Audit log of admin actions.
- On-call runbook (cross-references the ops runbook in B4 once V2
  ports it forward).

## 10. Production-only gaps

**Things Phase 4 didn't ship that V2 must:**

- HTTPS termination (TLS certs, renewal). Phase 4's `vergil-service`
  binds HTTP only.
- Per-environment config (dev / staging / prod database URLs, API
  keys, feature flags).
- CDN for the dashboard (when it exists).
- Per-tenant data isolation guarantees (schema-per-tenant vs
  row-level-security vs application-level filtering).
- GDPR / SOC 2 / whatever compliance work the enterprise customers
  ask for.

---

**Cross-reference:** every Phase 4 ADR ends with a "V2 swap path"
section pointing at the relevant item here. Update both when scope
changes.
