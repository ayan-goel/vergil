// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AMM} from "../src/AMM.sol";

// Hand-written Halmos check_ functions for the constant-product AMM.
// We target three weaker properties than the canonical "x*y >= k"
// constant — the full curve invariant requires multiplying two symbolic
// uint256 reserves, which is a known-hard case for symbolic execution
// over EVM uint256 arithmetic. The properties below are linear over
// the relevant variables and probe the same invariants the AMM is
// supposed to maintain:
//
//   1. swap_does_not_drain_pool — after any single swap, reserveY > 0
//      (the swap requires amountOut < ry, so this is the post-condition).
//   2. mint_increases_totalSupply — successful mint() leaves
//      totalSupply strictly greater than before.
//   3. burn_reduces_totalSupply — successful burn() leaves totalSupply
//      strictly less than before.
//
// See README for the AMM postmortem on the full curve invariant.
contract Properties {
    AMM internal token;

    constructor() {
        token = new AMM(1_000_000, 1_000_000);
    }

    /// A successful swap leaves a non-zero Y reserve.
    function check_swap_does_not_drain_pool(uint256 amountIn) external {
        // Bound amountIn so amountIn * 997 doesn't overflow the uint256
        // multiplication inside swap.
        require(amountIn > 0 && amountIn <= type(uint128).max);
        try token.swapXForY(amountIn) returns (uint256) {
            assert(token.reserveY() > 0);
        } catch {
            // Swap may revert (bad output, empty pool); only assert on success.
        }
    }

    /// A successful mint increases totalSupply.
    function check_mint_increases_totalSupply(
        uint256 depositX,
        uint256 depositY,
        address to
    ) external {
        require(to != address(0));
        require(depositX > 0 && depositX <= type(uint64).max);
        require(depositY > 0 && depositY <= type(uint64).max);
        uint256 before = token.totalSupply();
        try token.mint(depositX, depositY, to) returns (uint256) {
            assert(token.totalSupply() > before);
        } catch {
            // Mint may revert (proportionality, zero, etc.); only assert on success.
        }
    }

    /// A successful burn reduces totalSupply.
    function check_burn_reduces_totalSupply(uint256 shares, address to) external {
        require(to != address(0));
        require(shares > 0 && shares <= token.balanceOf(address(this)));
        uint256 before = token.totalSupply();
        try token.burn(shares, to) returns (uint256, uint256) {
            assert(token.totalSupply() < before);
        } catch {
            // Burn may revert (insufficient shares, zero out, etc.); only assert on success.
        }
    }

    /// The canonical x*y >= k invariant. EXPERIMENTAL — symbolic execution
    /// of multiplied uint256 reserves is a known-hard case for Halmos.
    /// Listed in properties.yaml so the runner picks it up; tracked in
    /// notes/phase3.md whether this verifies in practice.
    function check_swap_preserves_k_invariant(uint256 amountIn) external {
        // Tight bounds keep the multiplication tractable for the solver.
        require(amountIn > 0 && amountIn <= type(uint64).max);
        uint256 rxBefore = token.reserveX();
        uint256 ryBefore = token.reserveY();
        uint256 kBefore = rxBefore * ryBefore;
        try token.swapXForY(amountIn) returns (uint256) {
            uint256 kAfter = token.reserveX() * token.reserveY();
            assert(kAfter >= kBefore);
        } catch {
            // Swap may revert (bad output); only assert on success.
        }
    }

    // ─── Phase 4 Slice A6 — nonlinear AMM remediation attempts ──────────
    //
    // The canonical k-invariant above times out on Halmos because two
    // symbolic uint256 multiplications saturate the solver. The variants
    // below test the four remediation paths from
    // notes/phase3-amm-postmortem.md:
    //
    //   A6-path-1: tighter input bounds (uint32 amountIn)
    //   A6-path-2: decompose into directional monotonicity (no x*y)
    //   A6-path-4: per-reserve checks instead of the product
    //
    // Path 3 (cross-solver re-dispatch via A2) doesn't need a new
    // property — it re-runs the existing query through cvc5. Tested
    // separately via `vergil prove --solver cvc5`.

    /// Path 1: tighter input bounds. uint32 caps amountIn at ~4B which
    /// keeps the two multiplied reserves within ~10^15 — well within
    /// 64-bit arithmetic the solver handles cleanly.
    function check_swap_k_invariant_uint32_bounds(uint32 amountIn) external {
        require(amountIn > 0);
        uint256 rxBefore = token.reserveX();
        uint256 ryBefore = token.reserveY();
        uint256 kBefore = rxBefore * ryBefore;
        try token.swapXForY(uint256(amountIn)) returns (uint256) {
            uint256 kAfter = token.reserveX() * token.reserveY();
            assert(kAfter >= kBefore);
        } catch {}
    }

    /// Path 2 (directional decomposition): the swapXForY operation
    /// strictly increases reserveX and strictly decreases reserveY.
    /// No multiplications — Halmos handles this trivially.
    function check_swapXForY_moves_reserves_directionally(uint256 amountIn) external {
        require(amountIn > 0 && amountIn <= type(uint128).max);
        uint256 rxBefore = token.reserveX();
        uint256 ryBefore = token.reserveY();
        try token.swapXForY(amountIn) returns (uint256) {
            assert(token.reserveX() > rxBefore);
            assert(token.reserveY() < ryBefore);
        } catch {}
    }

    /// Path 4 (per-reserve check): conservation of value at single
    /// reserve granularity. The amount of X added equals what the
    /// AMM credited reserveX by. No products; verifier-friendly.
    function check_swap_reserveX_increments_by_amountIn(uint256 amountIn) external {
        require(amountIn > 0 && amountIn <= type(uint128).max);
        uint256 rxBefore = token.reserveX();
        try token.swapXForY(amountIn) returns (uint256) {
            assert(token.reserveX() == rxBefore + amountIn);
        } catch {}
    }
}
