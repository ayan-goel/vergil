# Getting started

This page takes you from zero to a first verified property in about
five minutes. The reference target is the bundled `examples/erc20`
contract.

## Prerequisites

You need three external toolchains on your `PATH`:

| Tool | Tested version | Install |
|---|---|---|
| `solc` | 0.8.20 | `solc-select use 0.8.20` |
| `halmos` | 0.3.3 | `pipx install halmos` |
| `foundry` (forge) | latest stable | `curl -L https://foundry.paradigm.xyz | bash` |

Optional, but recommended for the full LLM-guided flow:

| Tool | Purpose |
|---|---|
| `z3` / `cvc5` | SMT solvers Halmos and SMTChecker dispatch to |
| `slither` | Static analysis the manifest validator cross-checks |

## Install Vergil

For now Vergil is a workspace; install from source:

```bash
git clone https://example.com/vergil
cd vergil
cargo build --release --bin vergil
# binary lives at ./target/release/vergil
```

A `cargo install` recipe ships in Phase 4.

## Configure API keys (Phase 2 intent path only)

Skip this section if you only want the deterministic Phase 1 path
(`vergil verify <project>` against a checked-in `properties.yaml`).

For the natural-language intent path (`vergil verify <project> --intent
"..."`) you'll need two LLM provider keys. Vergil reads them from
environment variables (or a `.env` at the repo root):

```bash
export VERGIL_ANTHROPIC_API_KEY=sk-ant-...
export VERGIL_OPENAI_API_KEY=sk-...
# Optional — retrieval falls back to MockEmbedder when unset
export VOYAGE_API_KEY=...
```

## Your first verified property

```bash
cd examples/erc20
../../target/release/vergil verify .
```

You should see:

```
Vergil verify — project: /Users/.../examples/erc20
2 properties

  ✓ check_transfer_preserves_total_supply — verified by halmos in 30ms
  ✓ check_approve_idempotent — verified by halmos in 20ms

Summary: 2 verified, 0 counterexample, 0 unknown, 0 error
```

The `vergil-out/proof.json` file records the run: source-file SHA-256s,
which backend verified each property, an optional SMT-LIB query hash
for re-dispatch. You can re-check the proof later without re-running
Halmos:

```bash
../../target/release/vergil prove examples/erc20/proof.json
```

## A first intent-driven run

Skip ahead if you set up the API keys above:

```bash
../../target/release/vergil verify examples/erc20 --intent "Transfers \
move value without creating or destroying it. totalSupply changes only \
via mint or burn."
```

Vergil synthesizes 4 candidate properties, an independent critic
prunes the vacuous ones, the survivors run through the portfolio, and
you get the same shape of output. Typical cost: $0.50–$1.

## Next steps

- [Concepts](./concepts.md) — sound vs complete, the two-axis trust
  hierarchy that lets us trust an LLM-proposed spec.
- [Property catalog](./property-catalog.md) — 43 templates the
  synthesizer pulls from when proposing candidates.
- [CLI reference](./cli-reference.md) — every command, every flag.
- [FAQ](./faq.md) — why a proof beats a fuzz pass; how Vergil compares
  to Certora, ItyFuzz, and similar tools.
