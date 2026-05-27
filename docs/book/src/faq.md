# FAQ

## Why a proof, not a fuzz?

Tools like Foundry's `forge invariant` mode and ItyFuzz are excellent
at finding bugs — they generate inputs that trigger assertions.

When `forge invariant` reports no failures after N runs, you've shown
"the assertion held on N random samples." When Vergil reports
`verified`, you've shown "the assertion holds on every input the
encoded symbolic domain permits."

Both are valuable, and the tools complement each other:

- **Fuzz first** on new code to flush out obvious bugs cheaply.
- **Verify** once you think the property holds, to convert "no bugs
  found in 10K samples" into "no bugs exist in the domain we encoded."

Vergil isn't trying to replace fuzzing. It's adding a proof step on
top, so the green checkmark means more than a sample pass.

## How is Vergil different from Certora?

Certora's Prover is a mature, battle-tested formal verifier with a
spec language (CVL) and a commercial team behind it. Vergil shares
the same "spec language + solver" shape but differs in three ways:

1. **Specs come from natural language.** Vergil's Phase 2 path lets
   you describe what should hold in English; the LLM proposes the
   spec. Certora makes you write CVL by hand. The LLM is gated by
   the critique pass and mutation testing so it doesn't slip a
   vacuous spec past you.
2. **Open source, free to run.** The Rust workspace ships under
   Apache-2.0; the LLM API calls are the only cost (and you can
   disable them by using the Phase 1 deterministic path).
3. **Single-contract scope.** Phase 3 ships with single-contract
   verification. Multi-contract / cross-contract reasoning is Phase 4+
   territory; Certora handles this today.

If you're working on a high-stakes audit and your team writes specs,
Certora is the better choice today. If you're an individual developer
who wants to verify a property without learning CVL, Vergil's intent
path is a better fit.

## Why does the AMM `x*y >= k` invariant come back as `unknown`?

Multiplied symbolic `uint256` operands are a known frontier problem
for SMT solvers. The encoded query becomes combinatorially large and
hits the wall-clock budget before Z3 / cvc5 finds a model or proves
UNSAT.

The Slice 6 README + the `notes/phase3-amm-postmortem.md` document
the full diagnosis and Phase 4 remediation paths (tighter bounds,
bounded encoding, cross-solver re-dispatch via the captured SMT query
SHA, property decomposition).

The kernel handles the linear forms (swap doesn't drain the pool,
mint increases supply, burn reduces supply) just fine — those are
the three properties the AMM example reports as verified.

## The critique pass rejected my correct spec — what now?

Three options, in increasing order of effort:

1. **Tighten the intent.** The critic's rationale (in
   `vergil-out/trace/responses/*.txt`) usually points at what it
   thinks is loose. A more specific intent narrows the candidate
   space and gives the critic better calibration.
2. **Lower `min_axis` via the `--min-critique-axis` flag** (coming
   in Phase 4). The current 0.5 default is conservative; the kill
   criterion runs at 0.4 to similar effect.
3. **Switch critics.** Set `VERGIL_OPENAI_API_KEY` if you only have
   Anthropic configured (or vice versa). Same-provider critique is
   weaker than cross-provider; the run will warn you when it falls
   back.

## Can Vergil verify code I didn't write?

Yes — point `vergil verify` at any Foundry project with a `src/`
directory. The scaffold autodetects the first contract and generates
a default test rig calling its constructor with empty args. For
contracts whose constructor takes required args, write a custom
scaffold and pass it via `--scaffold`.

## What's planned for Phase 4?

- Multi-contract verification (Compound-style call graphs)
- Proxy / upgradeability invariants (storage slot stability)
- SMT-LIB re-dispatch through `vergil prove` (cross-solver retry)
- The remaining ~57 catalog templates
- The 100-contract VergilBench full run (deferred from Phase 3)
- Public scoreboard at vergilbench.org (or equivalent)
- `cargo install vergil-cli` + `brew install vergil` packaging
