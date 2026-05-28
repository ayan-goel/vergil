-- Phase 4 Slice C2: postgres schema mirror of the in-memory job store.
--
-- Shipped but NOT executed by Phase 4 — V2 wires this up against a real
-- postgres instance. The schema deliberately mirrors the JobStore trait
-- in crates/vergil-service/src/store.rs so V2 can drop in a postgres
-- impl without changing handler code.

CREATE TYPE job_status AS ENUM ('pending', 'running', 'completed', 'failed');

CREATE TABLE jobs (
    id              UUID PRIMARY KEY,
    tenant_id       TEXT NOT NULL,
    status          job_status NOT NULL DEFAULT 'pending',
    submitted_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    cost_usd        NUMERIC(10, 4),
    failure_reason  TEXT,

    -- Request payload denormalized for replay / audit.
    contract_source TEXT NOT NULL,
    intent          TEXT NOT NULL,
    properties_yaml TEXT,
    cost_budget_usd NUMERIC(10, 4),
    wall_clock_budget_seconds INTEGER
);

CREATE INDEX idx_jobs_tenant_status ON jobs (tenant_id, status);
CREATE INDEX idx_jobs_submitted_at  ON jobs (submitted_at DESC);

CREATE TABLE job_results (
    job_id UUID PRIMARY KEY REFERENCES jobs (id) ON DELETE CASCADE,
    proof  JSONB NOT NULL,
    stored_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Per-tenant cost accounting view, queried by the billing layer (V2).
CREATE VIEW tenant_cost_monthly AS
SELECT
    tenant_id,
    date_trunc('month', submitted_at) AS month,
    count(*) AS job_count,
    sum(cost_usd) FILTER (WHERE cost_usd IS NOT NULL) AS total_cost_usd
FROM jobs
GROUP BY tenant_id, date_trunc('month', submitted_at);
