# Vergil

Vergil is a formal verification tool for Solidity smart contracts. You
write a natural-language statement of what should be true; Vergil
synthesizes candidate properties via an LLM, an independent critic
rejects vacuous candidates, and a portfolio of symbolic execution
(Halmos) and CHC model checking (Solidity SMTChecker) either verifies
each surviving property or produces a concrete counterexample.

The trust hierarchy is explicit: **the LLM never decides correctness.**
A property is "verified" only when a sound solver (z3, cvc5, bitwuzla)
discharges the corresponding SMT query as UNSAT under the symbolic
domain the encoder explored. The LLM proposes; the solver disposes.

This book covers everything you need to use Vergil productively —
from a 5-minute install + first-verified-property walkthrough to the
full property catalog and the architectural decisions behind the
closed-loop pipeline.
