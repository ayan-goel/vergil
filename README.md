# Vergil

Mathematically verified Solidity smart contracts.

Vergil translates a Solidity contract and a statement of intent into formal properties, then proves them with a sound SMT-backed verifier. The output is either a machine-checkable proof certificate or a concrete Foundry test reproducing a counterexample.

## Install

```bash
brew install vergil
```

Or from source:

```bash
cargo install vergil-cli
```

Or as a Foundry dependency:

```bash
forge install vergil-tools/vergil
```

Vergil shells out to Foundry, Halmos, Slither, Gambit, Z3, and cvc5. Run `vergil doctor` to verify your toolchain.

## Quick start

```bash
cd my-foundry-project
vergil init
vergil verify src/Token.sol --intent "ERC-20 token; balances always sum to totalSupply"
```

Output lands in `vergil-out/`:

```
vergil-out/
├── report.md             Human-readable verification report
├── proof.json            Machine-checkable proof artifact
├── spec/                 Generated Halmos check functions and SMTChecker asserts
└── counterexamples/      Runnable Foundry tests for any violation
```

Exit codes:

- `0` — all properties verified.
- `1` — at least one counterexample found.
- `2` — at least one property returned `Unknown` or timed out.
- `3` — infrastructure error.

## How it works

1. **Static analysis.** `solc --storage-layout` provides authoritative storage slot identification; Slither extracts the call graph, modifiers, and inheritance.
2. **Spec synthesis.** An LLM proposes formal properties (Halmos `check_*` functions and SMTChecker assertions) using retrieval-augmented generation over a curated property catalog.
3. **Critique.** An independent LLM call filters vacuous specs. Gambit mutation testing scores remaining candidates.
4. **Verification.** Halmos discharges bounded per-function safety; SMTChecker CHC mode discharges unbounded multi-transaction invariants. Z3 and cvc5 are dispatched as a portfolio, escalating by theory.
5. **Refinement.** On counterexample, the LLM classifies the failure as code bug, spec bug, or ambiguous, then patches code or refines spec. Up to ten rounds.

The SMT solver is the trusted base. The LLM proposes; the solver decides. Every verified property in the report names the solver that discharged it and the static-analysis sources that validated its encoding.

## Commands

```
vergil verify <PATH>     Verify a contract against generated or supplied properties
vergil init              Scaffold Vergil config in a Foundry project
vergil prove <FILE>      Re-check an existing proof artifact, no LLM, no solver search
vergil bench             Run benchmark suites
vergil corpus update     Pull the latest property catalog
vergil doctor            Check that toolchain dependencies are installed
```

Run `vergil <command> --help` for full flag documentation.

## Configuration

`vergil init` writes a `vergil.toml`:

```toml
[llm]
primary = "anthropic/claude-opus-4-5"
critique = "openai/gpt-5"
samples = 16

[verify]
max_iterations = 10
solvers = ["z3", "cvc5"]
```

API keys come from `VERGIL_ANTHROPIC_API_KEY` and `VERGIL_OPENAI_API_KEY` environment variables.

## Documentation

Full documentation is at <https://vergil.tools/docs>.

## License

Apache 2.0. See [LICENSE-APACHE](LICENSE-APACHE).
