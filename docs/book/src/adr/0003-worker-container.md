# ADR 0003: Worker container — multi-stage Dockerfile, sandboxed subprocesses, multi-arch

**Status:** Accepted (2026-05-27) — Phase 4 Slice C3.
**Decider:** Phase 4 strategic pivot.

## Context

V2's worker pool spawns containerized verification runs. Each container
needs the entire Solidity toolchain (solc, halmos, slither, foundry,
z3, cvc5) plus the vergil binaries plus the example contracts for the
in-container smoke test. The image must build on amd64 + arm64 and
must not push to a public registry (proprietary posture).

## Decision

- Ship a multi-stage `Dockerfile` at the repo root:
  - `deps` stage installs the heavy subprocess deps (solc 0.8.20,
    halmos 0.3.3, slither 0.11.0, foundry v1.0.0, z3 from apt,
    cvc5 1.2.1 from upstream release).
  - `vergil-build` stage cargo-builds the workspace
    (`vergil`, `vergilbench`, `kill-criterion`).
  - `runtime` final stage combines them, sized for a worker pool.
- Base image: `debian:bookworm-slim`.
- Healthcheck runs `--version` on every required binary at build time.
- `ENTRYPOINT=vergil`, `CMD=doctor` so a bare `docker run` validates
  the image.
- `.dockerignore` excludes `target/`, `.git/`, `.env`, `tasks/`, `notes/`,
  `benchmarks/`, `vergil-out/`.

## Rationale

- **Multi-stage.** The deps stage is the slow one (~10 minutes uncached
  for solc + cvc5 + foundry + pipx installs). Multi-stage lets the
  vergil-build stage rebuild on every commit in ~30 seconds without
  reinstalling solc.
- **Pin every version.** `SOLC_VERSION=v0.8.20`, `FOUNDRY_RELEASE=v1.0.0`,
  `CVC5_VERSION=1.2.1`, `halmos==0.3.3`. Reproducibility > flexibility.
- **`debian:bookworm-slim`.** Smaller than ubuntu, glibc-based (alpine's
  musl trips foundry), apt has z3 + python3 + git out of the box.
- **No public registry push.** Phase 4 stays internal per the strategic
  pivot. V2 picks the registry (ECR / GHCR / GAR) when it deploys.
- **`vergil doctor` as default CMD.** Operators see "image is healthy"
  on a bare `docker run`. The healthcheck during build means a broken
  image fails to push at build time, not at deploy time.

## Multi-arch story

The Dockerfile uses `$(dpkg --print-architecture)` to pick the right
cvc5 + solc archives. Tested locally on Apple Silicon (arm64); amd64
verified via the Linux deps stage in CI (manual workflow).

Foundry's foundryup picks the right arch. Slither + halmos are pure
Python so they don't care.

## Subprocess sandbox status

The runtime image bundles the sandbox primitives (`sandbox-exec` ships
with macOS — irrelevant inside a Linux container; `bubblewrap` is **not**
installed in the image by default in Phase 4 — V2 enables it when the
sandbox wiring lands per B1's deferred follow-up). For Phase 4, ops can
treat the container as the sandbox boundary; V2 layers bubblewrap
inside the container for defense-in-depth.

## Consequences

- V2 picks the registry + the deployment infrastructure (item 3 of
  v2-readiness.md).
- Per-image disk footprint: ~1.4 GB compressed (debian + python +
  foundry + solc + cvc5 + z3 + slither + vergil binaries). Within the
  spec's "< 2 GB" target.
- Building locally: `docker build -t vergil-worker .` (multi-stage,
  one shot). ~15 minutes cold, ~30 seconds warm rebuild.
- Smoke test: `docker run --rm vergil-worker doctor` confirms every
  toolchain dep is wired.

## References

- `Dockerfile` at repo root.
- `.dockerignore` at repo root.
- ADR 0004 (auth) — service container uses the same image with a
  different CMD.
