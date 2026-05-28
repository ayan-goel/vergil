# ADR 0004: Auth model — `AuthProvider` trait, single-token stub, multi-tenant V2 swap path

**Status:** Accepted (2026-05-27) — Phase 4 Slice C4.
**Decider:** Phase 4 strategic pivot.

## Context

Every `vergil-service` endpoint needs an auth boundary. V2 will absolutely
do multi-tenant token-or-OIDC auth with per-tenant scopes; Phase 4 wants
a working middleware seam without dragging an identity provider into
the workspace.

## Decision

- Define an `AuthProvider` async trait in `vergil-service`. Sole
  method: `authenticate(auth_header: Option<&str>) -> Result<AuthIdentity, AuthError>`.
- `AuthIdentity` carries `tenant_id: String`. V2 wires the real value
  per customer.
- Phase 4 ships `SingleTokenAuth` that reads a token from
  `VERGIL_SERVICE_TOKEN` and accepts any request whose
  `Authorization: Bearer <token>` matches. Tenant ID defaults to
  `"internal"` (override via `with_tenant_id`).
- Auth wraps **every** endpoint, including `GET /v1/jobs` and `GET
  /v1/jobs/:id/result`. Only `/healthz` and `/metrics` are unauthenticated
  (per standard ops practice — these need to be reachable by load
  balancers / scrapers).

## Rationale

- **Trait at the seam.** V2 swaps the impl. The handler signature
  doesn't change; the middleware doesn't change. The bearer-token
  shape stays the same on the wire.
- **Env-var-driven config in Phase 4.** No config file, no rotation
  flow, no JWT verification. V2 wires the real flow when there's a
  real identity provider to authenticate against.
- **`tenant_id` flows through everything.** The CegisConfig has
  `tenant_id` (Slice B2); the telemetry event has `tenant_id` (Slice B2);
  the AuthIdentity has `tenant_id`. Threading is in place; only the
  source of truth (single env var → real per-customer record) changes
  in V2.

## V2 swap path

When V2 plugs in real multi-tenant auth, the change set is:

1. New `vergil-service/src/auth/<provider>.rs` implementing
   `AuthProvider`. Likely OIDC + JWT or a token table in postgres
   keyed by `tenant_id`.
2. Replace `SingleTokenAuth::new(env)` with the new constructor in
   the service entrypoint.
3. The `AuthIdentity { tenant_id }` flows the rest of the way unchanged.

No handler code changes. No CegisLoop changes. No telemetry sink
changes. The seam was the point.

## Consequences

- V2 picks the identity provider (item 2 of v2-readiness.md). Likely
  candidates: Auth0, Clerk, WorkOS, custom postgres-backed tokens.
- 401 vs 403 distinction: Phase 4 returns 401 for both "no token" and
  "wrong token". V2 may split into 401 (missing) vs 403 (insufficient
  scope) when it adds RBAC.
- Rate-limiting: Phase 4 has no per-token rate limit. V2 wires that
  separately (tower-governor or equivalent).

## References

- `crates/vergil-service/src/auth.rs`
- `openapi.yaml` — Bearer auth scheme declared.
- ADR 0002 (persistence) — `jobs.tenant_id` column maps to
  `AuthIdentity.tenant_id`.
