# Cetus Protocol (May 2024)

~$230M drained via faulty shift-overflow check in `checked_shlw`.
Cetus's bug accepted shift inputs that silently truncated high-order
bits, letting the attacker mint position liquidity at near-zero
asset cost.

**Maps to:** `arith-incorrect-overflow-check-shift`. The catalog
template was authored specifically for this Cetus pattern in Phase
1; the test verifies the template fires on a faithful reproduction.

**Post-mortem:** [Cetus official disclosure](https://x.com/CetusProtocol/status/1794193032081158615)
