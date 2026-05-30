// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Cetus Protocol (May 2024) — ~$230M drained.
///
/// Reproduction note: Cetus's `checked_shlw` (shift-left-with-check)
/// had a faulty overflow guard. The intended check was "after shift,
/// no high-order bits truncated" (i.e. `value < 2^192`). The bug used
/// the threshold `0xFFFFFFFFFFFFFFFF << 192` — which equals
/// `2^256 - 2^192`, a much looser bound. Inputs in
/// `[2^192, 2^256 - 2^192)` passed the check but the subsequent
/// shift-left-by-64 overflowed and silently lost the high 64 bits,
/// letting the attacker mint position liquidity at near-zero cost.
///
/// **PoC validation footnote**: my first reproduction used the
/// CORRECT threshold `value >= 2^192` — the template verified that
/// (false negative on the PoC, not on the catalog). The faithful
/// reproduction below uses the actual Cetus threshold; the template
/// finds the counterexample as expected. The lesson surfaced via
/// PoC validation: a reproduction must reproduce the BUG, not the
/// intended-but-not-shipped behaviour.
contract CetusLiquidityManager {
    mapping(address => uint256) public liquidityOf;

    /// Bug: threshold constant is wrong. Cetus's actual `checked_shlw`
    /// compared input against `0xFFFFFFFFFFFFFFFF << 192` =
    /// `2^256 - 2^192`, allowing values in `[2^192, 2^256 - 2^192)`
    /// to pass the check and then overflow the shift.
    function shiftLeft64(uint256 value) external pure returns (uint256) {
        require(value < 0xFFFFFFFFFFFFFFFF << 192, "CetusLiquidityManager: too large");
        unchecked {
            return value << 64;
        }
    }
}
