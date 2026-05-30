# Beanstalk Farms (Apr 2022)

~$182M drained via governance flash-loan attack. The attacker
flash-borrowed enough BEAN3CRV-f LP tokens to pass the
`emergencyCommit` supermajority quorum check, executed a malicious
proposal sweeping protocol reserves, then repaid the flash loan in
the same transaction.

**Maps to:** `flashloan-balance-dependent-state`. The defensive
property — "privileged gates consult a checkpointed snapshot, not
spot `balanceOf`" — is exactly what Beanstalk lacked.

**Post-mortem:** [Beanstalk governance exploit](https://bean.money/blog/beanstalk-governance-exploit)
