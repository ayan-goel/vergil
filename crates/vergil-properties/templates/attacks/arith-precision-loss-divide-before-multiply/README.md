# arith-precision-loss-divide-before-multiply

Detects `(a/c)*b` instead of `(a*b)/c` — integer division truncates the intermediate, biasing fees/shares/rewards. See `manifest.yaml` and `notes/attack-patterns.md` §3.3.
