# ADR 0001: Service API — HTTP+JSON with OpenAPI 3.x

**Status:** Accepted (2026-05-27) — Phase 4 Slice C1.
**Decider:** Phase 4 strategic pivot to proprietary-with-hosted-V2.
**Supersedes:** None (Phase 3 had no service layer).

## Context

V2 turns Vergil into a hosted enterprise SaaS. Phase 4 ships the V1 hardened
kernel plus a thin service skeleton so V2 can wrap a service around it in
weeks rather than months. The first decision is the wire protocol.

Three candidate shapes were on the table:

1. **HTTP + JSON** with an OpenAPI 3.x contract.
2. **gRPC** with protobuf.
3. **gRPC-Web** with a translation layer for browser clients.

## Decision

We ship **HTTP+JSON, OpenAPI 3.x**. The contract artifact is
`openapi.yaml` at the repo root, versioned under `/v1/` URL prefix.

## Rationale

- **Enterprise muscle memory.** Stripe, Linear, GitHub, OpenAI, Anthropic
  all expose HTTP+JSON. The teams that integrate Vergil already have
  curl-shaped intuition + tooling.
- **Browser-native.** V2's eventual web dashboard talks directly. No
  gRPC-Web bridge, no protoc-gen-js, no extra envoy hop.
- **Long-running jobs don't need streaming.** Verification jobs take
  seconds to hours; the right pattern is `POST /v1/jobs` → poll
  `GET /v1/jobs/:id`. gRPC's streaming superpower is wasted here, and
  the polling-friendly response time bins cleanly into HTTP caching.
- **Tooling parity.** `openapi-generator` produces typed clients in
  Python, TypeScript, Go, Ruby, Java without any extra work.
- **Easier to add gRPC later than vice versa.** When V2's internal
  worker → orchestrator RPC needs the perf, that path can go gRPC
  separately. The customer-facing edge stays HTTP+JSON.

## Counter-arguments considered

- **Streaming verdict updates.** gRPC server-streaming would let the
  client subscribe to a job's verdict transitions. We can ship Server-
  Sent Events (`text/event-stream`) on top of HTTP if/when it matters;
  no need to flip the whole protocol for one feature.
- **Schema strictness.** Protobuf is more rigorous than JSON Schema in
  some corners (oneof, well-known types). OpenAPI 3.1's JSON Schema
  draft-2020-12 covers our needs.
- **Bandwidth.** Protobuf is more compact, but our payloads are
  Solidity source + JSON proof artifacts in the hundreds of KB range,
  not microservice chatter. Negligible win.

## Consequences

- V2 inherits a stable contract: every endpoint is locked in
  `openapi.yaml`, and the Phase 4 stub handlers return the right JSON
  shape (with `501 not_implemented` until V2 plugs in the bodies).
- Customer SDKs can be generated on demand from the same artifact.
- Adding `/v2/` later is straightforward when we have V2 evolution data.
- The HTTP path picks up authentication, rate-limiting, and observability
  via stock middleware (axum / tower) rather than gRPC interceptors.

## References

- `openapi.yaml` at repo root.
- `crates/vergil-service/` — axum-based skeleton.
- ADR 0002 (persistence) + ADR 0004 (auth) build on this contract.
