# eip7702-delegate-arbitrary-execution

**document-only**. EIP-7702 SetCode-typed transactions install a delegate
contract's code on an EOA for the duration of one transaction. A hostile
delegate may execute arbitrary actions (token transfers, approvals) in the
EOA's context. No symbolic-execution encoding ships in V1.5 — analysis is
whole-program and Halmos doesn't model the EIP-7702 transaction type. The
catalog entry exists so dispatch can flag the pattern for human review. See
`manifest.yaml` and `notes/attack-patterns.md` §15.1.
