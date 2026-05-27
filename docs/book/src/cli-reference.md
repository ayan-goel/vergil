# CLI reference

Every `vergil` subcommand, its flags, and example invocations. This
page is the source of truth for `--help` output — Slice 14 of Phase 3
audited each command to match.

## `vergil verify`

Verify a Solidity project against a `properties.yaml` file (Phase 1)
or a natural-language intent (Phase 2).

```
vergil verify <path>
  [--properties <path>]      # default: <path>/properties.yaml
  [--format text|markdown|json]   # default: text
  [--intent "..."]           # opt into Phase 2 intent-driven path
  [--scaffold <path>]        # custom Halmos test scaffold
```

Examples:

```bash
# Phase 1: deterministic, uses properties.yaml
vergil verify examples/erc20

# Phase 2: natural-language intent
vergil verify examples/erc20 --intent "Transfers preserve totalSupply"

# Custom output format
vergil verify examples/erc20 --format markdown
```

Exit codes (SPEC §3.1):

| Code | Meaning |
|---|---|
| 0 | All properties verified |
| 1 | At least one counterexample |
| 2 | All resolved as unknown |
| 3 | Pipeline error (toolchain, IO, config) |

## `vergil prove`

Re-check a previously-emitted `proof.json` without re-running Halmos.
Verifies every source-file SHA-256 matches what was hashed at proof
time; Phase 4 will also re-dispatch each `smt_query_sha256`.

```
vergil prove <artifact.json>
```

## `vergil doctor`

Print every external tool's version + which were found on `PATH`.
First thing to run when verify fails with a build error.

```
vergil doctor
```

## `vergil init`

Scaffold a Vergil config in the current Foundry project.

```
vergil init    # writes properties.yaml + .gitignore entries
```

## `vergil bench` / `vergil corpus`

Bench infrastructure entry points — both stubs in Phase 1; the real
bench lives in the standalone `vergilbench` binary (see below).

## `vergilbench` (separate binary)

Run the VergilBench corpus.

```
vergilbench
  [--corpus <path>]     # default: vergilbench/
  [--max <n>]           # smoke-test the first n contracts
  [--vergil <path>]     # path to vergil binary
  [--verbose]
```

## Manual benchmarks

Per the Phase 3 manual-only-CI rule, none of these run on a schedule.
Use the GitHub workflow_dispatch UI or the Makefile targets.

```bash
make kill-criterion   # ~$12, ~22 min wall clock
make llm-live         # ~$0.05, smoke test
make bench            # zero-cost on the Phase 3 seed corpus
                       # ~$50-200 once the Phase 4 100-contract corpus lands
```

Every Makefile target prints estimated cost and waits for `y`
confirmation before running.
