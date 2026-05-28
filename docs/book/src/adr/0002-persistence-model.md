# ADR 0002: Persistence model — `JobStore` trait, in-memory default, postgres shipped not executed

**Status:** Accepted (2026-05-27) — Phase 4 Slice C2.
**Decider:** Phase 4 strategic pivot.

## Context

The service skeleton needs a place to persist jobs + results. V2 will
absolutely use a real database; Phase 4 doesn't want to drag postgres
into the workspace just yet (no migration runner, no docker-compose,
no test container — all of that is V2's posture-setting work).

## Decision

- Define a `JobStore` async trait in `vergil-service`. Methods:
  `submit`, `get`, `list`, `update_status`, `set_result`, `get_result`.
- Ship an `InMemoryStore` impl (`tokio::sync::RwLock<HashMap>`) for
  Phase 4 — the service skeleton's stub handlers, integration tests,
  and the worker container's smoke test all run against it.
- Ship `migrations/0001_jobs.sql` at the repo root but **do not run it**
  in Phase 4. The schema is the durable artifact; V2 wires up sqlx
  or diesel and runs the migration.

## Rationale

- **The trait is the seam.** V2 swaps the impl, not the call sites.
  Every handler that needs persistence depends on `Arc<dyn JobStore>`,
  not on a concrete type.
- **In-memory keeps Phase 4 testable.** No external dependency means
  the service skeleton ships green out of the box; ops can run
  `vergil-service` against `InMemoryStore` for smoke tests without a
  postgres container.
- **The schema is the contract.** Shipping `migrations/0001_jobs.sql`
  signs the table layout for V2 (and the V2 DBA / DevOps person) up
  front. If something needs to change in V2 (column names, indexes,
  partitioning strategy), the diff against this file is the audit
  trail.
- **`tenant_id` is in the schema even though Phase 4 doesn't multi-
  tenant.** V2 won't have to do a schema migration to add it.

## Schema details

`migrations/0001_jobs.sql` ships two tables + one view:

- `jobs` — primary record. Tenant ID, status enum, all the timestamps,
  contract source, intent, cost. Status enum covers
  `pending/running/completed/failed`.
- `job_results` — separated because proof.json artifacts are large
  (kilobytes to megabytes) and don't always need to be loaded with
  the job row. JSONB column for proof; nullable counterexample blob.
- `tenant_cost_monthly` view — pre-aggregated for billing. V2's
  billing job reads this directly.

## Consequences

- V2 picks up postgres in the V2 work plan (item 1 of v2-readiness.md).
- Cost: V2 ships a real persistence layer + migration runner + connection
  pool. Estimated 1-2 days of focused work for an engineer who's done
  it before.
- Risk: schema evolution from this baseline. Strategy: every Phase 5+
  schema change is its own migration file (`0002_...`, `0003_...`); no
  in-place edits to executed migrations.

## References

- `crates/vergil-service/src/store.rs`
- `crates/vergil-service/src/job.rs`
- `migrations/0001_jobs.sql`
